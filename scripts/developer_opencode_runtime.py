"""Provision the exact public OpenCode runtime pinned by the Developer runner."""

import hashlib
import os
from pathlib import Path, PurePosixPath
import platform
import shutil
import stat
import urllib.request
import uuid
import zipfile


VERSION = "1.18.23"
MAX_ARCHIVE_BYTES = 200 * 1024 * 1024
MAX_EXECUTABLE_BYTES = 200 * 1024 * 1024
RELEASE_BASE = f"https://github.com/anomalyco/opencode/releases/download/v{VERSION}"
PLATFORMS = {
    ("Darwin", "arm64"): {
        "archive": "opencode-darwin-arm64.zip",
        "archive_sha256": "373cf36673836f2ce8847295a0bb2cd2447d03c769b44d84185916bd471b4274",
        "executable": "opencode",
        "executable_sha256": "f7c45939a895e5a9febf141ab16307418bc41da31879aa0b2e65223190ca1c1a",
    },
    ("Windows", "AMD64"): {
        "archive": "opencode-windows-x64.zip",
        "archive_sha256": "a2fe9e8c2d074d26975024d494927b966680b3efdc3e0377eadb9afb05f7e191",
        "executable": "opencode.exe",
        "executable_sha256": "f831518278ded5090c41cc532b16ab80629e980f710a0b46d1e5b605808bb1d9",
    },
}


def sha256(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def download(url, destination):
    request = urllib.request.Request(url, headers={"User-Agent": "Assemblywright-E2E"})
    total = 0
    with urllib.request.urlopen(request, timeout=60) as response, destination.open("wb") as output:
        declared = response.headers.get("Content-Length")
        if declared is not None and int(declared) > MAX_ARCHIVE_BYTES:
            raise RuntimeError("Pinned OpenCode archive exceeds its download bound")
        while True:
            block = response.read(1024 * 1024)
            if not block:
                break
            total += len(block)
            if total > MAX_ARCHIVE_BYTES:
                raise RuntimeError("Pinned OpenCode archive exceeded its download bound")
            output.write(block)
    if total == 0:
        raise RuntimeError("Pinned OpenCode archive download was empty")


def extract_leaf(archive, destination, expected_name):
    with zipfile.ZipFile(archive) as package:
        files = [entry for entry in package.infolist() if not entry.is_dir()]
        if len(files) != 1:
            raise RuntimeError("Pinned OpenCode archive must contain exactly one file")
        entry = files[0]
        name = PurePosixPath(entry.filename)
        if (name.is_absolute() or len(name.parts) != 1 or name.name != expected_name
                or any(part in ("", ".", "..") for part in name.parts)):
            raise RuntimeError("Pinned OpenCode archive has an invalid executable path")
        mode = entry.external_attr >> 16
        if stat.S_ISLNK(mode) or entry.file_size <= 0 or entry.file_size > MAX_EXECUTABLE_BYTES:
            raise RuntimeError("Pinned OpenCode archive has an invalid executable entry")
        with package.open(entry) as source, destination.open("xb") as output:
            shutil.copyfileobj(source, output, length=1024 * 1024)


def provision_pinned_runtime(repository_root):
    key = (platform.system(), platform.machine())
    config = PLATFORMS.get(key)
    if config is None:
        raise RuntimeError(
            f"No pinned OpenCode {VERSION} E2E artifact is defined for {key[0]} {key[1]}")
    cache = (Path(repository_root).resolve() / "target" / "developer-fixtures" /
             f"opencode-{VERSION}" / f"{key[0].lower()}-{key[1].lower()}")
    cache.mkdir(parents=True, exist_ok=True)
    executable = cache / config["executable"]
    if executable.is_file() and not executable.is_symlink() \
            and sha256(executable) == config["executable_sha256"]:
        return executable
    nonce = uuid.uuid4().hex
    archive = cache / f"{config['archive']}.{nonce}.download"
    candidate = cache / f"{config['executable']}.{nonce}.candidate"
    try:
        download(f"{RELEASE_BASE}/{config['archive']}", archive)
        if sha256(archive) != config["archive_sha256"]:
            raise RuntimeError("Pinned OpenCode archive failed its fixed SHA-256 check")
        extract_leaf(archive, candidate, config["executable"])
        if sha256(candidate) != config["executable_sha256"]:
            raise RuntimeError("Pinned OpenCode executable failed its production SHA-256 check")
        candidate.chmod(0o700)
        os.replace(candidate, executable)
        return executable
    finally:
        archive.unlink(missing_ok=True)
        candidate.unlink(missing_ok=True)
