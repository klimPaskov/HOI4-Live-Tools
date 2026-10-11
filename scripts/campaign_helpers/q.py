import sys, json, urllib.request
tool, args, expr = sys.argv[1], sys.argv[2], sys.argv[3] if len(sys.argv) > 3 else ""
if args.startswith("@"): args = open(args[1:], encoding="utf-8").read()
req = urllib.request.Request("http://127.0.0.1:8765/call", data=json.dumps({"tool": tool, "args": json.loads(args), "timeout": 1500}).encode(), headers={"Content-Type": "application/json"})
out = json.loads(urllib.request.urlopen(req, timeout=1600).read().decode())
text = out["text"]
sys.stdout.reconfigure(encoding="utf-8")
i = text.find("\n{")
head, body = (text[:i], text[i+1:]) if i >= 0 else (text, "")
print(("[ERROR] " if out.get("isError") else "") + head)
if expr and body:
    try:
        d = json.loads(body[:body.rindex("}")+1])
        r = eval(expr, {"d": d})
        if isinstance(r, (list, tuple)):
            for x in r: print(x)
        elif r is not None: print(r)
    except Exception as e:
        print("filter error", e); print(body[:1500])
elif body:
    print(body[:3000])
