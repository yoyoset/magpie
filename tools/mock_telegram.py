"""Mock Telegram Bot API for testing Magpie's two-way Telegram without a real bot.

Run:  python tools/mock_telegram.py 8099
Then in config.toml:  [telegram] chat_id = 42, api_base = "http://127.0.0.1:8099"
and store any token (e.g. "123:TEST") under telegram-bot-token.

- Every call is appended to calls.jsonl as {"method", "body"}.
- getUpdates returns (once) whatever updates.json contains, then [] after a short wait.
"""
import json, os, sys, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

DIR = os.path.dirname(os.path.abspath(__file__))
CALLS = os.path.join(DIR, "calls.jsonl")
UPDATES = os.path.join(DIR, "updates.json")
msg_id = [100]


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def do_POST(self):
        method = self.path.rsplit("/", 1)[-1]
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
        with open(CALLS, "a", encoding="utf-8") as f:
            f.write(json.dumps({"method": method, "body": body}, ensure_ascii=False) + "\n")
        if method == "getUpdates":
            result = []
            if os.path.exists(UPDATES):
                with open(UPDATES, encoding="utf-8") as f:
                    result = [u for u in json.load(f) if u["update_id"] >= body.get("offset", 0)]
            if not result:
                time.sleep(1)
        elif method == "sendMessage":
            msg_id[0] += 1
            result = {"message_id": msg_id[0]}
        else:
            result = True
        out = json.dumps({"ok": True, "result": result}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(out)))
        self.end_headers()
        self.wfile.write(out)


if __name__ == "__main__":
    for p in (CALLS, UPDATES):
        if os.path.exists(p):
            os.remove(p)
    ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
