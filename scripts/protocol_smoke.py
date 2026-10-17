"""Offline stdio startup smoke: native permissions and a game are not needed."""
import json
import os
import subprocess
import sys
import tempfile


def main():
    with tempfile.TemporaryDirectory(prefix='hoi4-stdio-smoke-') as root:
        env = dict(os.environ, HOI4_LIVE_MODE='campaign', HOI4_LIVE_PLAY_MODE='fair',
                   HOI4_LIVE_ARTIFACTS=root, HOI4_LIVE_GAME_DIR=root,
                   HOI4_LIVE_USER_DIR=root, HOI4_LIVE_CONFIG=os.path.join(root, 'missing.json'))
        messages = [
            {'jsonrpc': '2.0', 'id': 1, 'method': 'initialize', 'params': {
                'protocolVersion': '2025-06-18', 'capabilities': {},
                'clientInfo': {'name': 'offline-smoke', 'version': '1'}}},
            {'jsonrpc': '2.0', 'id': 2, 'method': 'tools/list'},
            {'jsonrpc': '2.0', 'id': 3, 'method': 'tools/call', 'params': {
                'name': 'hoi4_console', 'arguments': {'commands': ['help']}}},
        ]
        proc = subprocess.run([sys.executable, '-m', 'hoi4_live'],
                              input=''.join(json.dumps(m) + '\n' for m in messages),
                              capture_output=True, text=True, env=env, timeout=30)
        if proc.returncode:
            raise RuntimeError('stdio startup failed: ' + proc.stderr[-2000:])
        # Every stdout line must be protocol JSON; even one diagnostic fails.
        lines = [json.loads(line) for line in proc.stdout.splitlines()]
        results = {line['id']: line for line in lines}
        assert results[1]['result']['serverInfo']['name'] == 'hoi4-live-tools'
        assert len(results[2]['result']['tools']) >= 43
        refused = results[3]['result']
        assert refused['isError'] and refused['structuredContent']['error'] == 'play_policy_refused'
        print('stdio initialize, tools/list and campaign refusal passed')


if __name__ == '__main__':
    main()
