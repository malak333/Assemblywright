use crate::PeerIdentityProfile;
use anyhow::{anyhow, bail, Context};
use core_foundation_sys::base::{kCFAllocatorDefault, Boolean, CFIndex, CFRelease, CFTypeRef};
use core_foundation_sys::data::{CFDataCreate, CFDataGetBytePtr, CFDataGetLength, CFDataRef};
use core_foundation_sys::dictionary::{
    kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks, CFDictionaryCreate,
    CFDictionaryGetValue, CFDictionaryRef,
};
use core_foundation_sys::number::{kCFNumberSInt64Type, CFNumberGetValue, CFNumberRef};
use core_foundation_sys::string::{kCFStringEncodingUTF8, CFStringCreateWithBytes, CFStringRef};
use std::ffi::c_void;
use std::ptr;
use std::sync::Mutex;

pub(crate) type PeerAuditToken = [u32; 8];

const ERR_SEC_SUCCESS: i32 = 0;
const K_SEC_CS_SIGNING_INFORMATION: u32 = 1 << 1;
const K_SEC_CS_NO_NETWORK_ACCESS: u32 = 1 << 29;
const K_SEC_CODE_SIGNATURE_RUNTIME: u64 = 0x10000;
const MAX_VERIFIED_PEER_AUDIT_TOKENS: usize = 64;

#[derive(Default)]
struct VerifiedPeerAuditTokens(Vec<PeerAuditToken>);

impl VerifiedPeerAuditTokens {
    fn contains(&self, token: &PeerAuditToken) -> bool {
        self.0.iter().any(|verified| verified == token)
    }

    fn remember(&mut self, token: PeerAuditToken) {
        if self.0.len() < MAX_VERIFIED_PEER_AUDIT_TOKENS && !self.contains(&token) {
            self.0.push(token);
        }
    }
}

/// A requirement is compiled once before the socket is bound and retained in
/// its stable binary form. Each blocking verification reconstructs an owned
/// Security-framework object, avoiding cross-thread CF object sharing.
pub(crate) struct CompiledPeerRequirement {
    bytes: Box<[u8]>,
    profile: PeerIdentityProfile,
    verified_tokens: Mutex<VerifiedPeerAuditTokens>,
}

impl CompiledPeerRequirement {
    pub(crate) fn compile(source: &str, profile: PeerIdentityProfile) -> anyhow::Result<Self> {
        let source = OwnedCf::new(unsafe {
            CFStringCreateWithBytes(
                kCFAllocatorDefault,
                source.as_ptr(),
                CFIndex::try_from(source.len()).context("peer requirement is too large")?,
                kCFStringEncodingUTF8,
                false as Boolean,
            ) as CFTypeRef
        })
        .ok_or_else(|| anyhow!("peer code requirement string allocation failed"))?;
        let mut requirement: CFTypeRef = ptr::null();
        let status = unsafe {
            SecRequirementCreateWithString(
                source.as_cf_string(),
                0,
                &mut requirement as *mut CFTypeRef,
            )
        };
        require_success(status, "peer code requirement compilation")?;
        let requirement = OwnedCf::new(requirement)
            .ok_or_else(|| anyhow!("peer code requirement compilation returned no object"))?;

        let mut data: CFDataRef = ptr::null();
        let status = unsafe {
            SecRequirementCopyData(requirement.as_type(), 0, &mut data as *mut CFDataRef)
        };
        require_success(status, "peer code requirement serialization")?;
        let data = OwnedCf::new(data as CFTypeRef)
            .ok_or_else(|| anyhow!("peer code requirement serialization returned no data"))?;
        let data_ref = data.as_cf_data();
        let length = unsafe { CFDataGetLength(data_ref) };
        if length <= 0 {
            bail!("peer code requirement serialization returned empty data");
        }
        let length = usize::try_from(length).context("peer code requirement data is too large")?;
        let bytes = unsafe { std::slice::from_raw_parts(CFDataGetBytePtr(data_ref), length) };
        Ok(Self {
            bytes: bytes.to_vec().into_boxed_slice(),
            profile,
            verified_tokens: Mutex::new(VerifiedPeerAuditTokens::default()),
        })
    }

    pub(crate) fn verify(&self, token: PeerAuditToken) -> anyhow::Result<()> {
        // LOCAL_PEERTOKEN includes the process-id generation. Holding this bounded
        // server-lifetime cache lock across the first Security.framework check both
        // prevents duplicate slow checks and keeps a new token fail closed.
        let mut verified_tokens = self
            .verified_tokens
            .lock()
            .map_err(|_| anyhow!("peer code identity cache is unavailable"))?;
        if verified_tokens.contains(&token) {
            return Ok(());
        }
        let token_data = OwnedCf::new(unsafe {
            CFDataCreate(
                kCFAllocatorDefault,
                token.as_ptr().cast::<u8>(),
                CFIndex::try_from(std::mem::size_of_val(&token))
                    .expect("audit token length fits CFIndex"),
            ) as CFTypeRef
        })
        .ok_or_else(|| anyhow!("peer audit token allocation failed"))?;
        let keys = [unsafe { kSecGuestAttributeAudit } as *const c_void];
        let values = [token_data.as_type()];
        let attributes = OwnedCf::new(unsafe {
            CFDictionaryCreate(
                kCFAllocatorDefault,
                keys.as_ptr(),
                values.as_ptr(),
                1,
                &kCFTypeDictionaryKeyCallBacks,
                &kCFTypeDictionaryValueCallBacks,
            ) as CFTypeRef
        })
        .ok_or_else(|| anyhow!("peer identity attribute allocation failed"))?;

        let mut code: CFTypeRef = ptr::null();
        let status = unsafe {
            SecCodeCopyGuestWithAttributes(
                ptr::null(),
                attributes.as_cf_dictionary(),
                0,
                &mut code as *mut CFTypeRef,
            )
        };
        require_success(status, "peer code lookup")?;
        let code = OwnedCf::new(code)
            .ok_or_else(|| anyhow!("peer code lookup returned no code object"))?;

        let compiled_data = OwnedCf::new(unsafe {
            CFDataCreate(
                kCFAllocatorDefault,
                self.bytes.as_ptr(),
                CFIndex::try_from(self.bytes.len()).context("peer requirement is too large")?,
            ) as CFTypeRef
        })
        .ok_or_else(|| anyhow!("peer code requirement data allocation failed"))?;
        let mut requirement: CFTypeRef = ptr::null();
        let status = unsafe {
            SecRequirementCreateWithData(
                compiled_data.as_cf_data(),
                0,
                &mut requirement as *mut CFTypeRef,
            )
        };
        require_success(status, "peer code requirement restoration")?;
        let requirement = OwnedCf::new(requirement)
            .ok_or_else(|| anyhow!("peer code requirement restoration returned no object"))?;

        let status = unsafe {
            SecCodeCheckValidity(
                code.as_type(),
                K_SEC_CS_NO_NETWORK_ACCESS,
                requirement.as_type(),
            )
        };
        require_success(status, "peer code identity validation")?;
        if self.profile == PeerIdentityProfile::DeveloperIdHardened {
            require_hardened_runtime(code.as_type())?;
        }
        verified_tokens.remember(token);
        Ok(())
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    #[test]
    fn verified_audit_token_cache_is_exact_deduplicated_and_bounded() {
        let mut cache = VerifiedPeerAuditTokens::default();
        let first = [1_u32; 8];
        cache.remember(first);
        cache.remember(first);
        assert!(cache.contains(&first));
        assert_eq!(cache.0.len(), 1);

        for value in 2..=MAX_VERIFIED_PEER_AUDIT_TOKENS as u32 {
            cache.remember([value; 8]);
        }
        assert_eq!(cache.0.len(), MAX_VERIFIED_PEER_AUDIT_TOKENS);
        let overflow = [u32::MAX; 8];
        cache.remember(overflow);
        assert!(!cache.contains(&overflow));
        assert_eq!(cache.0.len(), MAX_VERIFIED_PEER_AUDIT_TOKENS);
    }
}

fn require_hardened_runtime(code: CFTypeRef) -> anyhow::Result<()> {
    let mut information: CFDictionaryRef = ptr::null();
    let status = unsafe {
        SecCodeCopySigningInformation(
            code,
            K_SEC_CS_SIGNING_INFORMATION,
            &mut information as *mut CFDictionaryRef,
        )
    };
    require_success(status, "peer signing information validation")?;
    let information = OwnedCf::new(information as CFTypeRef)
        .ok_or_else(|| anyhow!("peer signing information returned no dictionary"))?;
    let flags = unsafe {
        CFDictionaryGetValue(
            information.as_cf_dictionary(),
            kSecCodeInfoFlags as *const c_void,
        )
    } as CFNumberRef;
    if flags.is_null() {
        bail!("peer signing information did not include code flags");
    }
    let mut value: i64 = 0;
    let converted = unsafe {
        CFNumberGetValue(
            flags,
            kCFNumberSInt64Type,
            (&mut value as *mut i64).cast::<c_void>(),
        )
    };
    if !converted || value < 0 || value as u64 & K_SEC_CODE_SIGNATURE_RUNTIME == 0 {
        bail!("peer code identity is not protected by the hardened runtime");
    }
    Ok(())
}

fn require_success(status: i32, operation: &str) -> anyhow::Result<()> {
    if status == ERR_SEC_SUCCESS {
        Ok(())
    } else {
        bail!("{operation} failed (OSStatus {status})")
    }
}

struct OwnedCf(CFTypeRef);

impl OwnedCf {
    fn new(value: CFTypeRef) -> Option<Self> {
        (!value.is_null()).then_some(Self(value))
    }

    fn as_type(&self) -> CFTypeRef {
        self.0
    }

    fn as_cf_string(&self) -> CFStringRef {
        self.0 as CFStringRef
    }

    fn as_cf_data(&self) -> CFDataRef {
        self.0 as CFDataRef
    }

    fn as_cf_dictionary(&self) -> CFDictionaryRef {
        self.0 as CFDictionaryRef
    }
}

impl Drop for OwnedCf {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) }
    }
}

#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    static kSecGuestAttributeAudit: CFStringRef;
    static kSecCodeInfoFlags: CFStringRef;

    fn SecCodeCopyGuestWithAttributes(
        host: CFTypeRef,
        attributes: CFDictionaryRef,
        flags: u32,
        guest: *mut CFTypeRef,
    ) -> i32;
    fn SecCodeCheckValidity(code: CFTypeRef, flags: u32, requirement: CFTypeRef) -> i32;
    fn SecCodeCopySigningInformation(
        code: CFTypeRef,
        flags: u32,
        information: *mut CFDictionaryRef,
    ) -> i32;
    fn SecRequirementCreateWithString(
        text: CFStringRef,
        flags: u32,
        requirement: *mut CFTypeRef,
    ) -> i32;
    fn SecRequirementCreateWithData(
        data: CFDataRef,
        flags: u32,
        requirement: *mut CFTypeRef,
    ) -> i32;
    fn SecRequirementCopyData(requirement: CFTypeRef, flags: u32, data: *mut CFDataRef) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requirement_compilation_is_fail_closed_and_redacted() {
        CompiledPeerRequirement::compile("true", PeerIdentityProfile::AdhocExact)
            .expect("compile valid requirement");
        let error = CompiledPeerRequirement::compile(
            "identifier == definitely invalid syntax",
            PeerIdentityProfile::AdhocExact,
        )
        .err()
        .expect("reject invalid requirement")
        .to_string();
        assert!(error.contains("OSStatus"), "{error}");
        assert!(!error.contains("definitely"), "{error}");
    }
}
