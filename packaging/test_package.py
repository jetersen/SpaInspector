import json
import os
import pathlib
import plistlib
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as ET

class PackagingTests(unittest.TestCase):
    def test_platform_manifests_and_version_validation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            binary = root / 'binary'
            binary.write_bytes(b'test binary')
            for platform in ('linux', 'windows', 'macos'):
                output = root / platform
                subprocess.run(['python3', 'packaging/package.py', platform, '--binary', str(binary), '--version', 'v1.2.3-beta.1', '--output', str(output)], check=True, capture_output=True)
                if platform == 'linux':
                    manifest = json.loads((output / 'flatpak.json').read_text())
                    self.assertNotIn('--share=network', manifest['finish-args'])
                    self.assertEqual(manifest['command'], 'spa-inspector')
                    if os.name != 'nt':
                        self.assertTrue((output / 'SpaInspector.AppDir/AppRun').stat().st_mode & 0o111)
                elif platform == 'windows':
                    package = ET.parse(output / 'installer.wxs').getroot()[0]
                    self.assertEqual(package.attrib['Scope'], 'perUser')
                    self.assertEqual(package.attrib['Version'], '1.2.3')
                else:
                    with (output / 'SPA Inspector.app/Contents/Info.plist').open('rb') as f:
                        self.assertEqual(plistlib.load(f)['CFBundleShortVersionString'], '1.2.3')
            result = subprocess.run(['python3', 'packaging/package.py', 'linux', '--binary', str(binary), '--version', '../bad', '--output', str(root / 'invalid')], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((root / 'invalid').exists())

if __name__ == '__main__':
    unittest.main()
