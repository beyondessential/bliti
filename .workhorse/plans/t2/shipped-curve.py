import csv
import json
from datetime import datetime, timezone

traces = json.load(open('50e-traces.json'))


def ts(s):
    return datetime.strptime(s, '%Y-%m-%dT%H:%M:%SZ').replace(tzinfo=timezone.utc).timestamp()


rows = list(csv.DictReader(open('/home/felix/bliti-v4-rundown/rundown.csv')))
t = [ts(r['utc']) for r in rows]
v = [float(r['volts']) for r in rows]
start = ts('2026-09-25T00:45:24Z')
end = t[-1]
run = end - start
print('run', run, 's', run / 3600, 'h; draw at 4000 mAh', 4.0 / (run / 3600), 'A')

# rolling median of 7 samples (70 s), then a running minimum so the fall is monotone
k = 3
med = []
for i in range(len(v)):
    win = sorted(v[max(0, i - k):i + k + 1])
    med.append(win[len(win) // 2])
mono = []
lo = 9.0
for x in med:
    lo = min(lo, x)
    mono.append(lo)


def remaining(volts):
    # share of the run left when the smoothed voltage first falls below `volts`
    for ti, vi in zip(t, mono):
        if vi < volts:
            return (end - ti) / run
    raise ValueError(volts)


tail_volts = [3.15, 3.1, 3.05, 3.0, 2.95, 2.9, 2.85, 2.8, 2.75, 2.7, 2.65, 2.6]
tail = [(2.571, 0.0)] + [(x, remaining(x)) for x in sorted(tail_volts)]
c32 = remaining(3.2)
tail.append((3.2, c32))
print('c(3.2)', c32, 'minutes left', c32 * run / 60)


def q_at(pts, volts):
    return max(a for a, vv in pts if vv >= volts)


# 0.75 A on a 5.5 Ah 58E is 0.136C; on the 4.9 Ah 50E that is 0.67 A, 0.34 of the way from 0.5 A to 1 A
wt = (0.75 / 5.5 * 4.9 - 0.5) / 0.5
print('blend weight toward 1 A', wt)


def f(volts):
    a = q_at(traces['0.5'], volts) / q_at(traces['0.5'], 3.2)
    b = q_at(traces['1.0'], volts) / q_at(traces['1.0'], 3.2)
    return a * (1 - wt) + b * wt


upper_volts = [3.25, 3.3, 3.35, 3.4, 3.45, 3.5, 3.55, 3.6, 3.65, 3.7, 3.8, 3.9, 4.0, 4.05, 4.1]
upper = [(x, c32 + (1 - c32) * (1 - f(x))) for x in upper_volts]
upper.append((4.2, 1.0))

points = [[round(a, 4), round(b, 4)] for a, b in tail + upper]
for p in points:
    print(p)
print(len(points), 'points')
doc = {
    'discharging': {
        'points': points,
        'learnt-from': 0,
        'error': 0.2,
        'duration': round(5.5 / 0.75 * 3600, 4),
    }
}
json.dump(doc, open('/tmp/58e/shipped-curve.json', 'w'))
