# Requiere PyGuitarPro: pip install pyguitarpro
import sys, os, glob
import guitarpro
root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
files = []
for pat in ('samples/*.gp3', 'samples/*.gp4', 'samples/*.gp5', 'samples/extra/*.gp3', 'samples/extra/*.gp4', 'samples/extra/*.gp5'):
    files += glob.glob(os.path.join(root, pat))
out = ['# Referencia generada con PyGuitarPro 0.11: archivo|pista|notas por compás (sin silencios)']
skipped = []
for f in sorted(files):
    rel = os.path.relpath(f, root).replace('\\', '/')
    try:
        s = guitarpro.parse(f)
    except Exception as e:
        skipped.append(rel)
        continue
    for ti, t in enumerate(s.tracks):
        counts = []
        for m in t.measures:
            n = 0
            for v in m.voices:
                for b in v.beats:
                    n += sum(1 for x in b.notes if x.type != guitarpro.NoteType.rest)
            counts.append(n)
        out.append(f'{rel}|{ti}|' + ','.join(map(str, counts)))
d = os.path.join(root, 'tests', 'fixtures')
os.makedirs(d, exist_ok=True)
open(os.path.join(d, 'pyguitarpro_notes.txt'), 'w', encoding='utf-8', newline='\n').write('\n'.join(out) + '\n')
print(len(files), 'archivos; omitidos:', skipped)

