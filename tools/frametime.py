import re,sys,statistics as st
# Median frame-time parts over world frames (>= N draws) of a run log.
N=int(sys.argv[2]) if len(sys.argv)>2 else 3000
rows=[]
for l in open(sys.argv[1],encoding='utf-8',errors='replace'):
    m=re.search(r'([\d.]+) fps, ([\d.]+) ms/frame \(([\d.]+) preparing: copies ([\d.]+) ms.*?; ([\d.]+) on the GPU; ([\d.]+) copying back.*?\), (\d+) draws/frame',l)
    if m and int(m.group(7))>=N: rows.append([float(x) for x in m.groups()])
if not rows: print('no world frames'); sys.exit()
k=['fps','ms','prep','copies','gpu','back','draws']
print(f'{len(rows)} samples:',', '.join(f'{n} {st.median(c):.0f}' if n!='fps' else f'{n} {st.median(c):.2f}' for n,c in zip(k,zip(*rows))))
