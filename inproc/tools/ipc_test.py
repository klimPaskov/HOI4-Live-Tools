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
check('opt-in: allow-listed name accepted, unknown name and empty refused; families named', first == dict(optin_ok=1, optin_bad=0, optin_empty=0, optin_families=1, family_of=[1, 2, -1, -1]))
check('parser: flat object ok; nesting, escapes, duplicates, floats refused',
      second == dict(parse_ok=3, parse_nested=-1, parse_escape=-1, parse_dup=-1, parse_float=-1))
for _ in range(50):
    if os.path.exists(sock + '.token') and os.path.exists(sock): break
    time.sleep(0.05)
# the server binds, then restricts the modes: wait up to 1 s for the restriction (the old single stat raced it)
modes = lambda: (oct(os.stat(sock).st_mode & 0o777), oct(os.stat(sock + '.token').st_mode & 0o777))
for _ in range(20):
    if modes() == ('0o600', '0o600'): break
    time.sleep(0.05)
check('socket and token are owner-only', modes() == ('0o600', '0o600'))
# wrong token: refused and disconnected
s = socket.socket(socket.AF_UNIX); s.connect(sock); s.sendall(b'{"v":1,"op":"hello","token":"00"}\n')
r = json.loads(s.recv(4096).split(b'\n')[0]); check('wrong token refused', r['ok'] is False and r['error'] == 'bad token')
check('wrong token closes the connection', s.recv(10) == b''); s.close()
# no hello: refused
s = socket.socket(socket.AF_UNIX); s.connect(sock); s.sendall(b'{"v":1,"op":"read"}\n')
r = json.loads(s.recv(4096).split(b'\n')[0]); check('requests before hello refused', r['error'] == 'hello first'); s.close()
ch = Channel(sock)
check('hello lists the allow-list (speed + families) and per-family state', ch.hello['post']['allow'] == ['set_game_speed', 'production', 'air', 'airwing', 'production2', 'orders', 'orders2', 'units', 'picks', 'events', 'decisions', 'diplomacy', 'holds', 'research', 'politics', 'construction', 'training', 'mio', 'navy', 'doctrine', 'projects', 'navyhq', 'move', 'raids', 'diplomacy2', 'leader'] and not ch.hello['families']['leader']['on'] and not ch.hello['families']['leader']['live_passed'] and not ch.hello['families']['diplomacy2']['on'] and not ch.hello['families']['diplomacy2']['live_passed'] and not ch.hello['families']['raids']['on'] and not ch.hello['families']['raids']['live_passed'] and not ch.hello['families']['move']['on'] and not ch.hello['families']['move']['live_passed'] and not ch.hello['families']['mio']['on'] and ch.hello['families']['mio']['live_passed'] and not ch.hello['families']['training']['on'] and not ch.hello['families']['training']['live_passed'] and not ch.hello['families']['construction']['on'] and ch.hello['families']['construction']['live_passed'] and not ch.hello['families']['politics']['on'] and not ch.hello['families']['research']['on'] and ch.hello['families']['research']['live_passed'] and not ch.hello['families']['holds']['on'] and ch.hello['families']['holds']['live_passed'] and ch.hello['families']['politics']['live_passed'] and not ch.hello['families']['navy']['on'] and ch.hello['families']['navy']['live_passed'] and not ch.hello['families']['doctrine']['on'] and not ch.hello['families']['doctrine']['live_passed'] and not ch.hello['families']['projects']['on'] and not ch.hello['families']['projects']['live_passed'] and not ch.hello['families']['diplomacy']['on'] and ch.hello['families']['diplomacy']['live_passed'] and not ch.hello['families']['decisions']['on'] and not ch.hello['families']['events']['on'] and not ch.hello['families']['events']['live_passed'] and not ch.hello['families']['picks']['on'] and ch.hello['families']['picks']['live_passed'] and not ch.hello['families']['units']['on'] and ch.hello['families']['units']['live_passed'] and ch.hello['families']['decisions']['live_passed'] and ch.hello['families']['orders2']['live_passed'] and not ch.hello['families']['orders2']['on'] and ch.hello['families']['orders']['live_passed'] and ch.hello['post']['enabled'] and ch.hello['families']['production']['on'] and not ch.hello['families']['air']['on'] and ch.hello['families']['production']['live_passed'] and ch.hello['families']['air']['live_passed'] and not ch.hello['families']['airwing']['on'] and ch.hello['families']['airwing']['live_passed'] and ch.hello['families']['production2']['live_passed'])
r = ch.call('post', cmd='air.transfer', objs='0x1', obj2='0x2'); check('stage 2: a family that is off is refused', 'family air off' in r.get('error', ''))
r = ch.call('post', cmd='console.move', objs='0x1'); check('stage 2: a family not on the allow-list is refused', r.get('error') == 'command not on the allow-list')
r = ch.call('post', cmd='production.add_factories', objs=''); check('stage 2: empty object list refused', 'objs' in r.get('error', ''))
r = ch.call('post', cmd='production.add_factories', objs=','.join(['0x1'] * 33), vals='1'); check('stage 2: more than 32 objects refused', 'objs' in r.get('error', ''))
r = ch.call('post', cmd='production.add_factories', objs='0x1,0x2,0x3', vals='1,2,3'); check('stage 2: a batch of three posted', r['ok'] and r['posted'] == 3)
r = ch.call('post', cmd='production.add_factories', objs='0x4', vals='1'); check('stage 2: a second request inside 50 ms rate-limited', 'rate limit' in r.get('error', ''))
time.sleep(0.06)
r = ch.call('post', cmd='production.add_factories', objs='0x1,0xbad', vals='1,1'); check('stage 2: a foreign object refused, reported per object', r['ok'] is False and r['refused'] == 1)
time.sleep(0.06)
r = ch.call('post', cmd='production.add_factories', objs='0x666', vals='1'); check('stage 2: unexplained refusal reported', r['ok'] is False)
time.sleep(0.06)
r = ch.call('post', cmd='production.add_factories', objs='0x1', vals='1'); check('stage 2: the family switched off after an unexplained refusal', 'family production off' in r.get('error', ''))
r = ch.call('post', cmd='set_research', speed=1); check('command off the allow-list refused', r['error'] == 'command not on the allow-list')
r = ch.call('post', cmd='set_game_speed', speed=5); check('speed above the scanned range refused', 'range' in r['error'])
r = ch.call('post', cmd='set_game_speed', speed=-1); check('negative speed refused', 'range' in r['error'])
r = ch.call('post', cmd='set_game_speed', speed='2'); check('string speed refused', 'range' in r['error'])
r = ch.call('post', cmd='set_game_speed', speed=2, extra=1); check('extra members refused', 'unexpected members' in r['error'])
r = ch.call('post', cmd='set_game_speed', speed=2); check('valid post executed', r['ok'] and r['speed_after'] == 2)
r = ch.call('post', cmd='set_game_speed', speed=1); check('second post inside 200 ms rate-limited', 'rate limit' in r['error'])
time.sleep(0.25)
r = ch.call('read'); check('read answers', r['ok'] and r['speed'] == 2)
r = ch.call('orders', group='0x1234'); check('the order-tree read answers (engineer 73)', r['ok'] and r['group'] == '0x1234' and r['lists'][0]['list'] == 'top')
r = ch.call('orders'); check('the order-tree read needs a group', 'group' in r.get('error', ''))
r = ch.call('post', cmd='orders.delete', objs='0x1', vals='5'); check('an orders post with the family not opted in is refused', 'family orders off' in r.get('error', ''))
r = ch.call('fronts', group='0x1234'); check('the theatre fronts read answers (engineer 74)', r['ok'] and r['fronts'][0]['sections'][0]['provinces'] == [11, 12])
r = ch.call('fronts'); check('the fronts read needs a group', 'group' in r.get('error', ''))
r = ch.call('mios'); check('the organisations read answers without an opt-in (engineer 84)', r['ok'] and r['organisations'][0]['key'] == 'ita_ansaldo' and r['organisations'][0]['traits'][0] == [7, 't', 'a'])
r = ch.call('diplomacy_actions'); check('the diplomacy catalog answers without an opt-in and lists unsupported shapes (engineer 122)', r['ok'] and r['actions'][0]['op'] == 'diplomacy2.embargo' and r['actions'][1]['shape'].startswith('unsupported:'))
r = ch.call('post', cmd='mio.policy', objs='0x10', vals='7'); check('mio posts need their family opted in', 'family mio off' in r.get('error', ''))
r = ch.call('post', cmd='orders2.assign', objs='0x1', vals='5', divs='0x2,0x3'); check('an orders2 post (divs member) with the family not opted in is refused', 'family orders2 off' in r.get('error', ''))
r = ch.call('post', cmd='units.new_army', objs='0x1', divs='0x1,0x2'); check('a units post with the family not opted in is refused', 'family units off' in r.get('error', ''))
r = ch.call('post', cmd='picks.focus', objs='0x1'); check('a picks post with the family not opted in is refused', 'family picks off' in r.get('error', ''))
r = ch.call('post', cmd='events.answer', objs='0x1', vals='0,2'); check('an events post with the family not opted in is refused', 'family events off' in r.get('error', ''))
r = ch.call('post', cmd='orders2.assign', objs='0x1', vals='5', divs=7); check('a numeric divs member is refused', 'member types' in r.get('error', '') or 'off' in r.get('error', ''))
ch.s.sendall(b'{"v":2,"op":"read","id":99}\n'); r = json.loads(ch.s.recv(4096).split(b'\n')[0])
check('other protocol versions refused', r['error'] == 'unsupported protocol version')
ch.s.sendall(b'not json\n'); r = json.loads(ch.s.recv(4096).split(b'\n')[0]); check('malformed line refused', r['error'] == 'malformed request')
r = ch.call('post', cmd='set_game_speed', speed=3); check('unexplained refusal reported', r['ok'] is False)
time.sleep(0.25)
r = ch.call('post', cmd='set_game_speed', speed=1); check('posting switched off after an unexplained refusal', 'posting off' in r.get('error', ''))
ch.s.sendall(b'{"v":1,"op":"read","pad":"' + b'x' * 21000 + b'"}\n'); time.sleep(0.2)
try: gone = ch.s.recv(10) == b''
except OSError: gone = True
check('a line over about 20 KB drops the client', gone)
ch.s.close()
# the stage-1 procedure end to end against the mock (fresh process: posting is on again)
open(sock + '.off', 'w').close(); p.wait(timeout=5); os.unlink(sock + '.off')
p = subprocess.Popen([os.path.join(build, 'ipc_selftest'), sock], stdout=subprocess.DEVNULL, stderr=open(os.path.join(d, 'log2'), 'w'),
                     env=dict(os.environ, IPC_SELFTEST_SPEED_ONLY='1'))
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
