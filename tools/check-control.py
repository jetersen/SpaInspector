#!/usr/bin/env python3
"""Exercise every MCP command against a running native or bridged browser app.

The check replaces the app's open files with the repository fixtures, changes its
view and preferences, then closes the fixtures. Use a dedicated test instance.
"""
import argparse
import base64
import json
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--port", type=int, default=8091)
parser.add_argument("--binary", type=Path, default=ROOT / ("target/debug/spa-control.exe" if os.name == "nt" else "target/debug/spa-control"))
parser.add_argument("--artifacts", type=Path, help="New directory for exports and screenshots")
args = parser.parse_args()
repo = ROOT
out = args.artifacts or Path(tempfile.mkdtemp(prefix="spa-control-check-"))
if args.artifacts:
    out.mkdir(parents=True, exist_ok=False)
p = subprocess.Popen(
    [str(args.binary), "--port", str(args.port), "mcp"],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True,
)
seq = 0
seen = set()


def rpc(method, params=None):
    global seq
    seq += 1
    p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": seq, "method": method,
                             "params": params or {}}) + "\n")
    p.stdin.flush()
    line = p.stdout.readline()
    assert line, "MCP process closed before replying"
    value = json.loads(line)
    assert value["id"] == seq, value
    assert "error" not in value, value
    return value["result"]


def call(method, arguments=None, expect_error=False):
    seen.add(method)
    value = rpc("tools/call", {"name": "spa_" + method.replace(".", "_"),
                               "arguments": arguments or {}})
    if expect_error:
        assert value["isError"], value
        return value
    assert not value["isError"], value
    return value.get("structuredContent", {})


def state():
    return call("state")


def wait(predicate):
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        value = state()
        if predicate(value):
            return value
        time.sleep(.02)
    raise AssertionError("App did not reach the expected state")


def find_node(label):
    found = [n for n in call("ui.elements")["nodes"] if n["label"] == label
             and ("Click" in n["actions"] or n["value"] is not None
                  or n["numericValue"] is not None)]
    return found[0] if found else None


def node(label):
    found = find_node(label)
    assert found is not None, label
    return found


def float32(value):
    return struct.pack("<f", value)


try:
    assert rpc('initialize', {'protocolVersion': '2025-11-25', 'capabilities': {}, 'clientInfo': {'name': 'smoke', 'version': '1'}})['protocolVersion'] == '2025-11-25'
    p.stdin.write(json.dumps({'jsonrpc': '2.0', 'method': 'notifications/initialized'}) + '\n')
    p.stdin.flush()
    tools = rpc('tools/list')['tools']
    assert any((t['name'] == 'spa_ui_screenshot' for t in tools))
    call('files.clear')
    call('view.set', {'mode': 'individual'})
    loaded = call('files.open', {'paths': [str(repo / 'tests/TestFiles/test1.spa'), str(repo / 'tests/TestFiles/test2.spa')]})
    assert len(loaded['files']) == 2
    assert loaded['files'][0]['series'][0]['points'] == 6846
    capabilities = call('capabilities')
    schemas = {c['method']: c for c in capabilities['commands']}
    assert schemas['files.open']['appInputSchema']['required'] == ['files']
    assert schemas['export']['appInputSchema']['required'] == ['format']
    doc = call('inspect', {'scope': 'document'})['data']
    assert 'sourceBytes' not in doc and 'y' not in doc['series'][0]
    raw = call('inspect', {'scope': 'section', 'index': 0, 'includePayload': True})['data']
    assert isinstance(raw['payload'], str)
    series = call('inspect', {'scope': 'series', 'includeData': True})['data']
    assert len(series['x']) == len(series['y']) == 6846
    obj = call('inspect', {'scope': 'object', 'index': 0})['data']
    assert 'classId' in obj
    call('files.select', {'file': 0, 'series': 1})
    assert state()['files'][0]['selectedSeries'] == 1
    call('plot.preset', {'section': '1500-700'}, expect_error=True)
    call('files.select', {'file': 0, 'series': 0})
    call('export', {'file': 0, 'format': 'json', 'output': str(out / 'original.json')})
    doc = json.loads((out / 'original.json').read_text())
    assert base64.b64decode(doc['sourceBytes']) == (repo / 'tests/TestFiles/test1.spa').read_bytes()
    call('export', {'file': 0, 'format': 'csv', 'output': str(out / 'original.csv')})
    call('export', {'file': 0, 'format': 'csv', 'output': str(out / 'original.csv')}, expect_error=True)
    call('plot.set_range', {'min': 700, 'max': 1500})
    interval = state()['plot']['range']
    assert interval[1] == 1500
    for section in ['4000-2500', '2500-1500', '1500-700']:
        call('plot.preset', {'section': section, 'preview': True})
        s = wait(lambda s: s['plot']['previewRange'] is not None)
        assert s['plot']['range'] == interval
        call('ui.screenshot', {'output': str(out / f'preset-{section}.png')})
    call('plot.preset', {'clearPreview': True})
    call('plot.zoom', {'factor': 0.5})
    assert state()['plot']['range'][1] - state()['plot']['range'][0] < interval[1] - interval[0]
    call('plot.sample', {'trace': 0, 'x': 1200})
    call('plot.reset')
    # Edit the real text widget, then activate its Apply button.
    field = node('X min')
    call('ui.perform', {'id': field['id'], 'action': 'setValue', 'value': '1000'})
    button = node('Apply interval')
    call('ui.perform', {'id': button['id'], 'action': 'click'})
    assert state()['plot']['range'][0] == 1000
    call('plot.reset')
    preset = node('1500–700')
    call('ui.perform', {'id': preset['id'], 'action': 'hover'})
    assert state()['plot']['previewRange'] is not None
    # Inspect the rendered selection while holding the graph pointer down.
    graph = state()['plot']['graphBounds']
    x = (graph[0] + graph[2]) / 2
    y = (graph[1] + graph[3]) / 2
    call('ui.perform', {'action': 'pointerMove', 'x': x, 'y': y})
    call('ui.perform', {'action': 'pointerDown', 'x': graph[0] + (graph[2] - graph[0]) * 0.25, 'y': y})
    call('ui.perform', {'action': 'pointerMove', 'x': graph[0] + (graph[2] - graph[0]) * 0.65, 'y': y})
    call('ui.screenshot', {'output': str(out / 'drag-band.png')})
    call('ui.perform', {'action': 'pointerUp', 'x': graph[0] + (graph[2] - graph[0]) * 0.65, 'y': y})
    assert state()['plot']['range'] != state()['plot']['domain']
    call('ui.perform', {'action': 'wheel', 'x': x, 'y': y, 'deltaY': 100})
    call('plot.reset')
    call('view.set', {'mode': 'overlay', 'includedFiles': [0, 1], 'xUnitCode': 1, 'yUnitCode': 17})
    s = wait(lambda s: len(s['plot']['traces']) == 2)
    graph = s['plot']['graphBounds']
    for element in call('ui.elements')['nodes']:
        if element['label'] in ['test1.spa', 'test2.spa'] and 'Click' in element['actions']:
            bounds = element['bounds']
            assert bounds and (bounds[2] <= graph[0] or bounds[0] >= graph[2]
                               or bounds[3] <= graph[1] or bounds[1] >= graph[3])
    call('plot.trace', {'trace': 1, 'visible': False})
    assert state()['plot']['visiblePoints'] == 6846
    call('plot.trace', {'trace': 1, 'visible': True})
    # Screenshot remains an MCP image as well as an exported PNG.
    image = rpc('tools/call', {'name': 'spa_ui_screenshot', 'arguments': {}})
    assert image['content'][0]['type'] == 'image'
    assert base64.b64decode(image['content'][0]['data']).startswith(b'\x89PNG')
    call('preferences', {'theme': 'light'})
    assert state()['effectiveTheme'] == 'light'
    call('ui.screenshot', {'output': str(out / 'light.png')})
    call('preferences', {'theme': 'system', 'encoding': 'latin1'})
    wait(lambda s: not s['busy'])
    call('preferences', {'encoding': 'auto'})
    wait(lambda s: not s['busy'])
    call('files.close', {'file': 1})
    assert len(state()['files']) == 1
    call('files.clear')
    assert not state()['files']
    assert {t['name'] for t in tools} == {'spa_' + m.replace('.', '_') for m in seen}
    print(json.dumps({'port': args.port, 'commandsCovered': len(seen), 'commands': sorted(seen), 'artifacts': str(out), 'result': 'passed'}))
finally:
    p.stdin.close()
    try:
        p.wait(timeout=5)
    except subprocess.TimeoutExpired:
        p.kill()
        p.wait()
