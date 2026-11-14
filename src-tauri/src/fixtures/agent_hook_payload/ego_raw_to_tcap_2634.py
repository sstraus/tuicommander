"""Convert the ego 0.4.3 native question raw PTY log into TUICCAP2.

raw.pty keeps the output bytes only, so chunk boundaries are reconstructed:
one output record ends after each OSC 7770 frame. The answer Enter goes in
just before the modal_end frame; `/quit` goes after the turn_end frame.
Cursor-position replies to ego's DSR queries are not replayed.
"""
import base64, json, re, struct, sys
raw = open(sys.argv[1], 'rb').read()
out_path = sys.argv[2]
frames = list(re.finditer(rb'\x1b\]7770;([^\x07\x1b]*)(\x07|\x1b\\)', raw))
cuts, inputs = [], {}
for m in frames:
    body = m.group(1)
    event = None
    if body.startswith(b'agent-event='):
        event = json.loads(base64.b64decode(body[len(b'agent-event='):]))['event']
    if event == 'modal_end':
        cuts.append(m.start()); inputs[m.start()] = b'\r'
    cuts.append(m.end())
    if event == 'turn_end':
        inputs[m.end()] = b'/quit\r'
cuts = sorted(set(c for c in cuts if 0 < c < len(raw))) + [len(raw)]
rec, prev, t = [], 0, 0
for c in cuts:
    t += 10_000
    rec.append((0, t, raw[prev:c])); prev = c
    if c in inputs:
        t += 10_000; rec.append((1, t, inputs[c]))
buf = bytearray(b'TUICCAP2\n') + struct.pack('<HH', 32, 110)
for d, us, data in rec:
    buf += struct.pack('<BQI', d, us, len(data)) + data
open(out_path, 'wb').write(bytes(buf))
print(len(rec), 'records')
