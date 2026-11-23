"""ipc_test.py BUILD_DIR: the channel and posting policy against the mock game (ipc_selftest). Offline; no game."""
import json, os, socket, subprocess, sys, tempfile, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from inproc_client import Channel
build = sys.argv[1]; fails = []
def check(name, cond):
    print(('PASS ' if cond else 'FAIL ') + name)
    if not cond: fails.append(name)
d = tempfile.mkdtemp(); os.chmod(d, 0o700); sock = os.path.join(d, 's.sock')
p = subprocess.Popen([os.path.join(build, 'ipc_selftest'), sock], stdout=subprocess.PIPE, stderr=open(os.path.join(d, 'log'), 'w'), text=True)
first = json.loads(p.stdout.readline()); second = json.loads(p.stdout.readline())
check('opt-in: allow-listed name accepted, unknown name and empty refused', first == dict(optin_ok=1, optin_bad=0, optin_empty=0))
check('parser: flat object ok; nesting, escapes, duplicates, floats refused',
      second == dict(parse_ok=3, parse_nested=-1, parse_escape=-1, parse_dup=-1, parse_float=-1))
for _ in range(50):
    if os.path.exists(sock + '.token') and os.path.exists(sock): break
    time.sleep(0.05)
check('socket and token are owner-only', oct(os.stat(sock).st_mode & 0o777) == '0o600' and oct(os.stat(sock + '.token').st_mode & 0o777) == '0o600')
# wrong token: refused and disconnected
s = socket.socket(socket.AF_UNIX); s.connect(sock); s.sendall(b'{"v":1,"op":"hello","token":"00"}\n')
r = json.loads(s.recv(4096).split(b'\n')[0]); check('wrong token refused', r['ok'] is False and r['error'] == 'bad token')
check('wrong token closes the connection', s.recv(10) == b''); s.close()
# no hello: refused
s = socket.socket(socket.AF_UNIX); s.connect(sock); s.sendall(b'{"v":1,"op":"read"}\n')
r = json.loads(s.recv(4096).split(b'\n')[0]); check('requests before hello refused', r['error'] == 'hello first'); s.close()
ch = Channel(sock)
check('hello lists the one allowed command', ch.hello['post']['allow'] == ['set_game_speed'] and ch.hello['post']['enabled'])
r = ch.call('post', cmd='set_research', speed=1); check('command off the allow-list refused', r['error'] == 'command not on the allow-list')
r = ch.call('post', cmd='set_game_speed', speed=5); check('speed above the scanned range refused', 'range' in r['error'])
r = ch.call('post', cmd='set_game_speed', speed=-1); check('negative speed refused', 'range' in r['error'])
r = ch.call('post', cmd='set_game_speed', speed='2'); check('string speed refused', 'range' in r['error'])
r = ch.call('post', cmd='set_game_speed', speed=2, extra=1); check('extra members refused', 'unexpected members' in r['error'])
r = ch.call('post', cmd='set_game_speed', speed=2); check('valid post executed', r['ok'] and r['speed_after'] == 2)
r = ch.call('post', cmd='set_game_speed', speed=1); check('second post inside 200 ms rate-limited', 'rate limit' in r['error'])
time.sleep(0.25)
r = ch.call('read'); check('read answers', r['ok'] and r['speed'] == 2)
ch.s.sendall(b'{"v":2,"op":"read","id":99}\n'); r = json.loads(ch.s.recv(4096).split(b'\n')[0])
check('other protocol versions refused', r['error'] == 'unsupported protocol version')
ch.s.sendall(b'not json\n'); r = json.loads(ch.s.recv(4096).split(b'\n')[0]); check('malformed line refused', r['error'] == 'malformed request')
r = ch.call('post', cmd='set_game_speed', speed=3); check('unexplained refusal reported', r['ok'] is False)
time.sleep(0.25)
r = ch.call('post', cmd='set_game_speed', speed=1); check('posting switched off after an unexplained refusal', 'posting off' in r.get('error', ''))
ch.s.sendall(b'{"v":1,"op":"read","pad":"' + b'x' * 2000 + b'"}\n'); time.sleep(0.2)
try: gone = ch.s.recv(10) == b''
except OSError: gone = True
check('a line over 1 KiB drops the client', gone)
ch.s.close()
# the stage-1 procedure end to end against the mock (fresh process: posting is on again)
open(sock + '.off', 'w').close(); p.wait(timeout=5); os.unlink(sock + '.off')
p = subprocess.Popen([os.path.join(build, 'ipc_selftest'), sock], stdout=subprocess.DEVNULL, stderr=open(os.path.join(d, 'log2'), 'w'))
for _ in range(50):
    if os.path.exists(sock): break
    time.sleep(0.05)
open(sock + '.speed', 'w').write('{"speed":0}\n')
client = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'inproc_client.py')
pr = subprocess.run([sys.executable, '-I', client, sock, 'stage1', d, '--external', 'cat ' + sock + '.speed'], capture_output=True, text=True); rc = pr.returncode
if rc: print(pr.stdout[-1500:], pr.stderr[-1500:])
res = json.load(open(os.path.join(d, 'stage1-result.json')))
check('stage-1 procedure passes end to end against the mock (40 repetitions, read-backs, undo)', rc == 0 and res['pass_'] and len(res['repetitions']) == 40)
open(sock + '.off', 'w').close(); p.wait(timeout=5)
check('kill switch closes the channel and removes the socket', p.returncode == 0 and not os.path.exists(sock))
print('IPC ALL PASS' if not fails else 'IPC FAILED: %s' % fails)
sys.exit(1 if fails else 0)
