#!/usr/bin/env python3
"""Build and open the production-profile supervised Developer app."""

import importlib.util
from pathlib import Path
import sys


sys.dont_write_bytecode = True


SOURCE = Path(__file__).with_name("developer-build.py")
SPEC = importlib.util.spec_from_file_location("assemblywright_developer_build", SOURCE)
DEVELOPER_BUILD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(DEVELOPER_BUILD)


if __name__ == "__main__":
    DEVELOPER_BUILD.main(DEVELOPER_BUILD.PRODUCTION_BUILD_PROFILE)
