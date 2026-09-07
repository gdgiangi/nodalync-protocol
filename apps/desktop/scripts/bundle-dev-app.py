#!/usr/bin/env python3
"""Wrap the already-built native development executable for macOS app discovery."""
from pathlib import Path
import os
import plistlib
import shutil
import struct
import subprocess
import sys
import tempfile

root = Path(__file__).resolve().parents[1]
profile = Path(sys.argv[1] if len(sys.argv) > 1 else Path(tempfile.gettempdir()) / 'nodalync-studio-dev-profile').resolve()
binary = root / 'target/debug/nodalync-desktop'
if not binary.is_file():
    raise SystemExit('Build first: cargo +1.98.0 build --locked --no-default-features')
app = root / 'target/debug/bundle/macos/Nodalync Studio.app'
contents = app / 'Contents'
macos = contents / 'MacOS'
resources = contents / 'Resources'
macos.mkdir(parents=True, exist_ok=True)
resources.mkdir(parents=True, exist_ok=True)


def replace_file(destination, source=None, data=None, mode=None):
    """Replace a bundle file without truncating an executable that is running."""
    descriptor, temporary_name = tempfile.mkstemp(prefix='.studio-', dir=destination.parent)
    os.close(descriptor)
    temporary = Path(temporary_name)
    try:
        if source is not None:
            shutil.copy2(source, temporary)
        else:
            temporary.write_bytes(data)
            temporary.chmod(0o644)
        if mode is not None:
            temporary.chmod(mode)
        os.replace(temporary, destination)
    finally:
        temporary.unlink(missing_ok=True)


replace_file(macos / 'nodalync-desktop', source=binary)
# Keep the exact existing PNG pixels; add only an ICNS container for Launch Services.
png = (root / 'icons/128x128.png').read_bytes()
entry = b'ic07' + struct.pack('>I', len(png) + 8) + png
replace_file(resources / 'studio.icns', data=b'icns' + struct.pack('>I', len(entry) + 8) + entry)
launcher = macos / 'nodalync-studio'
quoted_profile = "'" + str(profile).replace("'", "'\\''") + "'"
launcher_contents = '#!/bin/sh\nexport NODALYNC_DATA_DIR=' + quoted_profile + '\nexport NODALYNC_GRAPH_DB="$NODALYNC_DATA_DIR/studio/knowledge.db"\nexec "$(dirname "$0")/nodalync-desktop" "$@"\n'
replace_file(launcher, data=launcher_contents.encode(), mode=0o755)
plist = {
    'CFBundleName': 'Nodalync Studio',
    'CFBundleDisplayName': 'Nodalync Studio',
    'CFBundleIdentifier': 'com.nodalync.desktop',
    'CFBundleExecutable': 'nodalync-studio',
    'CFBundlePackageType': 'APPL',
    'CFBundleShortVersionString': '0.1.0',
    'CFBundleVersion': '0.1.0',
    'CFBundleIconFile': 'studio.icns',
    'LSMinimumSystemVersion': '10.15',
    'NSHighResolutionCapable': True,
    'NSPrincipalClass': 'NSApplication',
    'NSAppTransportSecurity': {'NSAllowsLocalNetworking': True},
}
replace_file(contents / 'Info.plist', data=plistlib.dumps(plist))
register = Path('/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister')
if register.exists():
    subprocess.run([str(register), '-f', str(app)], check=True)
print(app)
print('Development profile:', profile)
print('Bundle registered; app not launched.')
