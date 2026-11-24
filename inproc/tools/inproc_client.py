#!/usr/bin/env python3
"""inproc_client.py: the MCP side of the in-process module's local channel (protocol v1), and the stage-1 procedure.

  inproc_client.py SOCK hello|read|probe                  one request, prints the reply
  inproc_client.py SOCK post SPEED [ID]                   set_game_speed(SPEED), SPEED = speed index 0..4 (game 1..5)
  inproc_client.py SOCK post2 OP OBJS [--vals V,..] [--obj2 ADDR] [--flag 0|1]
        stage 2, one family op over 1..32 own objects (hex addresses, comma-separated): production.add_factories
        (--vals one delta per line), production.remove_line, air.transfer (--obj2 the CAirBase), air.set_mission
        (--vals one mission mask, --flag 1 enable / 0 disable), air.enable_mission (--obj2 the CStrategicRegion).
        The module checks every object with the game's resolver and owner getter, then the game's own validity check.
  inproc_client.py SOCK orders GROUP
        engineer 73: the battle-plan order tree of one own army or army group (hex address, e.g. from hoi4_armies'
        debug words or the reader's group list), read by the module between two frames
  inproc_client.py SOCK post2 orders.OP GROUP --vals ORDER_ID [--provs P,P,..] [--obj2 DRAWER] [--flag 0|1]
        orders.delete / orders.execute (--vals 0 = the whole plan; --flag 1 execute, 0 stop) / orders.set_path /
        orders.new_front (--provs the provinces in drawing order; --obj2 the drawing group, default the group)
  inproc_client.py SOCK stage1 OUTDIR --external CMD [--shot CMD]
        Part A of the stage-1 session: hello, probe, one post and its read-back by the module AND by the external
        runtime (CMD prints hoi4_overview's JSON body), undo by the same route, 20 timed repetitions. Writes
        OUTDIR/stage1-result.json (pass_ and every check). --shot CMD is run with the shot name appended
        (screenshots of the game's own speed indicator for the record).
  inproc_client.py SOCK soak OUTDIR --minutes M [--every S] [--hook MINUTE:CMD ...]
        Part B: once every S seconds (default 60) post speed index 3 then 4 (game speed 4, back to 5), each read
        back; a hook command runs once at its minute (e.g. a manual save). Stops at the first failure. Writes
        OUTDIR/stage1-soak.jsonl and OUTDIR/stage1-soak-result.json.

The socket is authenticated by the module: same user (checked by the module) plus the token it writes to SOCK.token.
Speeds are the game's speed index (0..4), the same numbers hoi4_overview and hoi4_time use."""
import json, os, socket, statistics, subprocess, sys, time


class Channel:
    def __init__(self, sock_path, timeout=10.0):
        with open(sock_path + '.token') as f:
            token = f.read().strip()
        self.s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.s.settimeout(timeout)
        self.s.connect(sock_path)
        self.buf = b''
        self.next_id = 1
        self.last_post = 0.0
        self.hello = self.call('hello', token=token)
        if not self.hello.get('ok'):
            raise RuntimeError('hello refused: %s' % self.hello.get('error'))

    def call(self, op, **kw):
        rid = self.next_id; self.next_id += 1
        req = dict(v=1, op=op, id=rid, **kw)
        self.s.sendall((json.dumps(req, separators=(',', ':')) + '\n').encode())
        while b'\n' not in self.buf:
            chunk = self.s.recv(65536)
            if not chunk:
                raise RuntimeError('channel closed by the module')
            self.buf += chunk
        line, self.buf = self.buf.split(b'\n', 1)
        rep = json.loads(line)
        if rep.get('id') != rid:
            raise RuntimeError('reply id %r for request %r' % (rep.get('id'), rid))
        return rep

    def post_speed(self, speed):
        # keep above the module's minimum interval between posts (it refuses closer ones)
        gap = self.hello.get('post', {}).get('min_interval_ms', 200) / 1000 + 0.05 if hasattr(self, 'hello') else 0.25
        wait = self.last_post + gap - time.monotonic()
        if wait > 0:
            time.sleep(wait)
        self.last_post = time.monotonic()
        t0 = time.perf_counter()
        rep = self.call('post', cmd='set_game_speed', speed=int(speed))
        rep['client_ms'] = round((time.perf_counter() - t0) * 1000, 2)
        return rep


def external_speed(cmd):
    t0 = time.perf_counter()
    out = subprocess.run(cmd, shell=True, capture_output=True, text=True, timeout=90).stdout
    body = json.loads(out)
    body = body.get('body', body)
    return body, round((time.perf_counter() - t0) * 1000, 1)


def pct(xs, p):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(round(p / 100 * (len(xs) - 1))))] if xs else None


def stage1(ch, outdir, external, shot):
    res = dict(stage=1, hello=ch.hello, checks={}, steps=[])
    c = res['checks']
    c['post_enabled'] = ch.hello.get('post', {}).get('enabled') is True
    # stage 2: the allow-list names the speed command and the families; the speed command is the only one opted in
    # for this procedure (the launcher's HOI4_INPROC_POST=set_game_speed), so no family may be on
    fams = ch.hello.get('families', {})
    c['allow_list_is_one_command'] = (ch.hello.get('post', {}).get('allow', [None])[0] == 'set_game_speed' and
                                      not any(f.get('on') for f in fams.values()))
    probe = ch.call('probe'); res['probe'] = probe
    c['probe_main_thread'] = probe.get('main_thread') is True
    c['probe_single_player'] = probe.get('server_type') == '12CDummyServer'
    c['probe_session_type'] = probe.get('queue_type') == '8CSession'
    c['probe_would_post'] = probe.get('would_post') is True
    before = ch.call('read'); res['before'] = before
    if not all(c.values()) or not before.get('ok'):
        return finish(res, outdir, 'stopped before posting: a pre-check failed')
    s0 = before['speed']; target = 2 if s0 != 2 else 1
    # one post, read back by the module, by the external runtime, and on screen
    rep = ch.post_speed(target); res['steps'].append(dict(step='post', **rep))
    c['post_executed'] = rep.get('ok') is True and rep.get('result') == 'executed'
    c['module_readback'] = rep.get('speed_after') == target
    ext, ext_ms = external_speed(external)
    res['steps'].append(dict(step='external_after_post', speed=ext.get('speed'), paused=ext.get('paused'), ms=ext_ms))
    c['external_readback'] = ext.get('speed') == target
    if shot: subprocess.run(shot + ' stage1-after-post', shell=True, timeout=60)
    # undo by the same route
    rep = ch.post_speed(s0); res['steps'].append(dict(step='undo', **rep))
    c['undo_executed'] = rep.get('ok') is True and rep.get('speed_after') == s0
    ext, ext_ms = external_speed(external)
    res['steps'].append(dict(step='external_after_undo', speed=ext.get('speed'), ms=ext_ms))
    c['external_after_undo'] = ext.get('speed') == s0
    if shot: subprocess.run(shot + ' stage1-after-undo', shell=True, timeout=60)
    if not all(c.values()):
        return finish(res, outdir, 'stopped after the first post: a check failed')
    # 20 repetitions (post and undo each time = 40 posts), spaced above the module's 200 ms minimum
    reps = []
    for i in range(20):
        for v in (target, s0):
            time.sleep(0.3)
            r = ch.post_speed(v)
            reps.append(dict(i=i, speed=v, ok=r.get('ok'), result=r.get('result'), post_us=r.get('post_us'),
                             main_wait_ms=r.get('main_wait_ms'), readback_ms=r.get('readback_ms'),
                             total_ms=r.get('total_ms'), client_ms=r.get('client_ms'), requeues=r.get('requeues'),
                             reason=r.get('reason')))
            if not r.get('ok'):
                break
        if not reps[-1]['ok']:
            break
    res['repetitions'] = reps
    c['repetitions_40_of_40'] = len(reps) == 40 and all(r['ok'] for r in reps)
    good = [r for r in reps if r['ok']]
    res['timing'] = {k: dict(median=statistics.median([r[k] for r in good]), p95=pct([r[k] for r in good], 95),
                             max=max(r[k] for r in good))
                     for k in ('post_us', 'main_wait_ms', 'readback_ms', 'total_ms', 'client_ms')} if good else {}
    # "one frame-time budget": the module sees the change within two frames at 60 fps (33 ms) at the median
    if good:
        c['readback_within_frame_budget'] = res['timing']['readback_ms']['median'] <= 33.4
    after = ch.call('read'); res['after'] = after
    c['speed_restored'] = after.get('speed') == s0
    return finish(res, outdir, 'done')


def finish(res, outdir, note):
    res['note'] = note
    res['pass_'] = bool(res['checks']) and all(res['checks'].values())
    with open(os.path.join(outdir, 'stage1-result.json'), 'w') as f:
        json.dump(res, f, indent=1)
    print(json.dumps(dict(pass_=res['pass_'], note=note, checks=res['checks'], timing=res.get('timing')), indent=1))
    return 0 if res['pass_'] else 1


def soak(ch, outdir, minutes, every, hooks):
    end = time.time() + minutes * 60
    start = time.time(); n = 0; fails = []; done_hooks = set()
    log = open(os.path.join(outdir, 'stage1-soak.jsonl'), 'a')
    while time.time() < end:
        minute = (time.time() - start) / 60
        for m, cmd in hooks:
            if minute >= m and m not in done_hooks:
                done_hooks.add(m)
                rc = subprocess.run(cmd, shell=True, timeout=600).returncode
                log.write(json.dumps(dict(t=time.time(), hook=cmd, rc=rc)) + '\n'); log.flush()
        for v in (3, 4):
            r = ch.post_speed(v); n += 1
            rec = dict(t=time.time(), minute=round(minute, 2), speed=v, ok=r.get('ok'), result=r.get('result'),
                       post_us=r.get('post_us'), readback_ms=r.get('readback_ms'), main_wait_ms=r.get('main_wait_ms'),
                       reason=r.get('reason'), error=r.get('error'))
            log.write(json.dumps(rec) + '\n'); log.flush()
            if not r.get('ok'):
                fails.append(rec)
                break
            time.sleep(0.5)
        if fails:
            break
        rd = ch.call('read')
        log.write(json.dumps(dict(t=time.time(), read=rd)) + '\n'); log.flush()
        time.sleep(max(0.0, every - 1.0))
    res = dict(posts=n, failures=fails, minutes=round((time.time() - start) / 60, 1), hooks_run=sorted(done_hooks),
               pass_=not fails)
    with open(os.path.join(outdir, 'stage1-soak-result.json'), 'w') as f:
        json.dump(res, f, indent=1)
    print(json.dumps(res))
    return 0 if not fails else 1


def main(argv):
    if len(argv) < 3:
        print(__doc__); return 2
    sock, op = argv[1], argv[2]
    ch = Channel(sock)
    if op in ('hello',):
        print(json.dumps(ch.hello, indent=1)); return 0
    if op in ('read', 'probe'):
        print(json.dumps(ch.call(op), indent=1)); return 0
    if op == 'post':
        print(json.dumps(ch.post_speed(int(argv[3])), indent=1)); return 0
    if op == 'orders':
        print(json.dumps(ch.call('orders', group=argv[3]), indent=1)); return 0
    if op == 'post2':
        # stage 2, one family op: post2 OP OBJS [--vals V,V] [--obj2 ADDR] [--flag 0|1]
        # e.g. post2 production.add_factories 0x6000abc0 --vals 5 ; post2 air.transfer 0x6001,0x6002 --obj2 0x7000
        rest = argv[5:]
        kw = {'cmd': argv[3], 'objs': argv[4]}
        for k in ('vals', 'obj2', 'provs'):
            if '--' + k in rest: kw[k] = rest[rest.index('--' + k) + 1]
        if '--flag' in rest: kw['flag'] = int(rest[rest.index('--flag') + 1])
        print(json.dumps(ch.call('post', **kw), indent=1)); return 0
    if op == 'stage1':
        outdir = argv[3]; ext = shot = None
        rest = argv[4:]
        if '--external' in rest: ext = rest[rest.index('--external') + 1]
        if '--shot' in rest: shot = rest[rest.index('--shot') + 1]
        if not ext:
            print('stage1 needs --external CMD'); return 2
        return stage1(ch, outdir, ext, shot)
    if op == 'soak':
        outdir = argv[3]; rest = argv[4:]
        minutes = float(rest[rest.index('--minutes') + 1]) if '--minutes' in rest else 30
        every = float(rest[rest.index('--every') + 1]) if '--every' in rest else 60
        hooks = []
        for i, a in enumerate(rest):
            if a == '--hook':
                m, cmd = rest[i + 1].split(':', 1); hooks.append((float(m), cmd))
        return soak(ch, outdir, minutes, every, hooks)
    print(__doc__); return 2


if __name__ == '__main__':
    sys.exit(main(sys.argv))
