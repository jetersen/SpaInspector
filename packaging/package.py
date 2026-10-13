"""Stage desktop packages from a release binary; run on the target OS."""
import argparse
import json
import pathlib
import plistlib
import re
import shutil
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument('platform', choices=['linux', 'windows', 'macos'])
parser.add_argument('--binary', required=True, type=pathlib.Path)
parser.add_argument('--version', required=True)
parser.add_argument('--output', default='artifacts/package', type=pathlib.Path)
a = parser.parse_args()
version = a.version.removeprefix('v')
if not re.fullmatch(r'\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?', version):
    parser.error('version must be a three-part semantic version')
numeric = version.split('-')[0]
root = a.output
root.mkdir(parents=True, exist_ok=True)
app_id = 'io.github.jetersen.SpaInspector'
if a.platform == 'windows':
    shutil.copy2(a.binary, root / 'spa-inspector.exe')
    shutil.copy2('LICENSE.md', root / 'LICENSE')
    wxs = f'''<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">
<Package Name="SPA Inspector" Manufacturer="jetersen" Version="{numeric}" UpgradeCode="B8400F43-95DB-4B9D-BB9E-4944638A9256" Scope="perUser">
<MajorUpgrade DowngradeErrorMessage="A newer version is already installed."/><MediaTemplate EmbedCab="yes"/>
<StandardDirectory Id="LocalAppDataFolder"><Directory Id="Programs" Name="Programs"><Directory Id="INSTALLFOLDER" Name="SPA Inspector">
<Component Id="Application"><File Id="InspectorExe" Source="spa-inspector.exe"/><File Source="LICENSE"/>
<RegistryValue Root="HKCU" Key="Software\\jetersen\\SpaInspector" Name="Installed" Type="integer" Value="1" KeyPath="yes"/></Component>
</Directory></Directory></StandardDirectory>
<StandardDirectory Id="ProgramMenuFolder"><Directory Id="MenuFolder" Name="SPA Inspector"><Component Id="Menu">
<Shortcut Id="Launch" Name="SPA Inspector" Target="[#InspectorExe]" WorkingDirectory="INSTALLFOLDER"/>
<RemoveFolder Id="RemoveMenu" On="uninstall"/><RegistryValue Root="HKCU" Key="Software\\jetersen\\SpaInspector" Name="Shortcut" Type="integer" Value="1" KeyPath="yes"/>
</Component></Directory></StandardDirectory>
<Feature Id="Main"><ComponentRef Id="Application"/><ComponentRef Id="Menu"/></Feature>
</Package></Wix>'''
    (root / 'installer.wxs').write_text(wxs)
elif a.platform == 'macos':
    contents = root / 'SPA Inspector.app' / 'Contents'
    (contents / 'MacOS').mkdir(parents=True, exist_ok=True)
    (contents / 'Resources').mkdir(exist_ok=True)
    shutil.copy2(a.binary, contents / 'MacOS' / 'spa-inspector')
    shutil.copy2('LICENSE.md', contents / 'Resources' / 'LICENSE')
    with (contents / 'Info.plist').open('wb') as f:
        plistlib.dump(dict(CFBundleName='SPA Inspector', CFBundleDisplayName='SPA Inspector', CFBundleIdentifier=app_id,
            CFBundleExecutable='spa-inspector', CFBundlePackageType='APPL', CFBundleShortVersionString=numeric,
            CFBundleVersion=numeric, LSMinimumSystemVersion='11.0', NSHighResolutionCapable=True), f)
    (root / 'Applications').symlink_to('/Applications', target_is_directory=True)
else:
    appdir = root / 'SpaInspector.AppDir'
    bindir = appdir / 'usr/bin'
    bindir.mkdir(parents=True, exist_ok=True)
    shutil.copy2(a.binary, bindir / 'spa-inspector')
    desktop = f'[Desktop Entry]\nType=Application\nName=SPA Inspector\nExec=spa-inspector\nIcon={app_id}\nCategories=Science;Education;\nTerminal=false\n'
    (appdir / f'{app_id}.desktop').write_text(desktop)
    icon = '<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" viewBox="0 0 256 256"><rect width="256" height="256" rx="40" fill="#17212b"/><path d="M40 40V216H220M40 175L80 173L105 170L124 90L139 166L165 158L183 55L197 145L220 130" fill="none" stroke="#44cbd6" stroke-width="10" stroke-linejoin="round"/></svg>'
    (appdir / f'{app_id}.svg').write_text(icon)
    (appdir / '.DirIcon').symlink_to(f'{app_id}.svg')
    (appdir / 'AppRun').write_text('#!/bin/sh\nexec "${APPDIR:-$(dirname "$0")}/usr/bin/spa-inspector" "$@"\n')
    (appdir / 'AppRun').chmod(0o755)
    manifest = dict(id=app_id, runtime='org.freedesktop.Platform', **{'runtime-version':'25.08'}, sdk='org.freedesktop.Sdk', command='spa-inspector',
        **{'finish-args':['--share=ipc','--socket=wayland','--socket=fallback-x11','--device=dri']},
        modules=[dict(name='spa-inspector', buildsystem='simple', **{'build-commands':[
            'install -Dm755 usr/bin/spa-inspector /app/bin/spa-inspector',
            f'install -Dm644 {app_id}.desktop /app/share/applications/{app_id}.desktop',
            f'install -Dm644 {app_id}.svg /app/share/icons/hicolor/scalable/apps/{app_id}.svg']}, sources=[dict(type='dir', path='SpaInspector.AppDir')])])
    (root / 'flatpak.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(root)
