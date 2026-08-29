import Foundation
import Testing
@testable import AssemblywrightMacCore

@Suite("Repository onboarding receipt")
struct RepositoryOnboardingReceiptTests {
    @Test("Strict decoder accepts the exact path-free Windows receipt")
    func acceptsExactReceipt() throws {
        let receipt = try AssemblywrightMacRepositoryOnboardingReceipt.decodeStrict(validReceipt())

        #expect(receipt.repositoryID.uuidString.lowercased() == repositoryID)
        #expect(receipt.registrationGrantRevision == 1)
        #expect(receipt.cloudDisclosureGrantRevision == 1)
        #expect(receipt.autonomousPublicationGrantRevision == 1)
        #expect(receipt.baseBranch == "main")
        #expect(receipt.headCommit == String(repeating: "a", count: 40))
        #expect(receipt.scopeSHA256 == String(repeating: "b", count: 64))
        #expect(receipt.approvalPlanSHA256 == String(repeating: "c", count: 64))
        #expect(receipt.preflightFingerprintSHA256 == String(repeating: "d", count: 64))
    }

    @Test("Strict decoder rejects malformed and extra-key receipts")
    func rejectsMalformedAndExtraKeyReceipts() {
        expectInvalid(Data(#"{"schema_version":1"#.utf8))
        expectInvalid(validReceipt(extra: #", "repository_path":"C:\\private""#))
    }

    @Test("Strict decoder rejects duplicate keys")
    func rejectsDuplicateKeys() {
        expectInvalid(
            validReceipt(extra: #", "repository_id":"bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb""#)
        )
    }

    @Test("Strict decoder rejects noncanonical case")
    func rejectsNoncanonicalCase() {
        expectInvalid(validReceipt(repositoryID: repositoryID.uppercased()))
        expectInvalid(validReceipt(headCommit: String(repeating: "A", count: 40)))
        expectInvalid(validReceipt(scopeSHA256: String(repeating: "B", count: 64)))
    }

    @Test("Strict decoder rejects nil identity and zero authority metadata")
    func rejectsNilAndZeroValues() {
        let nullIdentity = String(decoding: validReceipt(), as: UTF8.self).replacingOccurrences(
            of: #""repository_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa""#,
            with: #""repository_id":null"#
        )
        expectInvalid(Data(nullIdentity.utf8))
        expectInvalid(validReceipt(repositoryID: "00000000-0000-0000-0000-000000000000"))
        expectInvalid(validReceipt(registrationRevision: "0"))
        expectInvalid(validReceipt(registrationRevision: "2"))
        expectInvalid(validReceipt(scopeSHA256: String(repeating: "0", count: 64)))
        expectInvalid(validReceipt(approvalPlanSHA256: String(repeating: "0", count: 64)))
        expectInvalid(validReceipt(preflightSHA256: String(repeating: "0", count: 64)))
    }

    @Test("Strict decoder rejects oversized input before JSON parsing")
    func rejectsOversizedInput() {
        let data = Data(repeating: 0x20, count: AssemblywrightMacRepositoryOnboardingReceipt.maximumBytes + 1)
        #expect(throws: AssemblywrightMacRepositoryOnboardingReceiptError.tooLarge) {
            try AssemblywrightMacRepositoryOnboardingReceipt.decodeStrict(data)
        }
    }

    private let repositoryID = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"

    private func validReceipt(
        repositoryID: String? = nil,
        registrationRevision: String = "1",
        headCommit: String? = nil,
        scopeSHA256: String? = nil,
        approvalPlanSHA256: String? = nil,
        preflightSHA256: String? = nil,
        extra: String = ""
    ) -> Data {
        Data(
            """
            {"schema_version":1,"status":"repository_onboarding_ready","repository_id":"\(repositoryID ?? self.repositoryID)","registration_grant_revision":\(registrationRevision),"cloud_disclosure_grant_revision":1,"autonomous_publication_grant_revision":1,"base_branch":"main","head_commit":"\(headCommit ?? String(repeating: "a", count: 40))","scope_sha256":"\(scopeSHA256 ?? String(repeating: "b", count: 64))","approval_plan_sha256":"\(approvalPlanSHA256 ?? String(repeating: "c", count: 64))","preflight_fingerprint_sha256":"\(preflightSHA256 ?? String(repeating: "d", count: 64))"\(extra)}
            """.utf8
        )
    }

    private func expectInvalid(_ data: Data) {
        #expect(throws: AssemblywrightMacRepositoryOnboardingReceiptError.invalid) {
            try AssemblywrightMacRepositoryOnboardingReceipt.decodeStrict(data)
        }
    }
}
