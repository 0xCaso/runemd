#!/usr/bin/env python3
# Contrasto dell'anello "sta scrivendo" sui pixel (DESIGN.md, "Contrasto"): per ogni punto dei lati
# dritti attorno alla scheda (rect in pt dal --tree, scala 2) il pixel più contrastato sui 3 px di
# spessore, contro la barra delle schede (panel) e contro il riempimento sel della scheda.
# Lo spessore è quello dell'anello (`fx::ring::RING_W`, 2,5 pt) a scala 2: 5 px (prima 3, con l'anello di 1,5).
# uso: python3 -I bench/ring_contrast.py FOTO.png X Y W H [BANDA_PX]   (es. 258 16 240 28 da bench/worst_case.sh)
import subprocess, sys, statistics
img, rx, ry, rw, rh = sys.argv[1], *map(float, sys.argv[2:6])
band = int(sys.argv[6]) if len(sys.argv) > 6 else 5
scale = 2
W, H = map(int, subprocess.check_output(['ffprobe','-v','error','-select_streams','v:0','-show_entries','stream=width,height','-of','csv=p=0',img]).decode().strip().split(','))
raw = subprocess.check_output(['ffmpeg','-v','error','-i',img,'-f','rawvideo','-pix_fmt','rgb24','-'])
def px(x, y):
    i = (y*W + x)*3
    return raw[i:i+3]
def lum(c):
    def ch(v):
        v /= 255
        return v/12.92 if v <= 0.04045 else ((v+0.055)/1.055)**2.4
    r,g,b = map(ch, c)
    return 0.2126*r + 0.7152*g + 0.0722*b
def ratio(a, b):
    la, lb = lum(a), lum(b)
    return (max(la,lb)+0.05)/(min(la,lb)+0.05)
x0, y0, x1, y1 = int(rx*scale), int(ry*scale), int((rx+rw)*scale), int((ry+rh)*scale)
panel = px(int(x1 + 40), int((y0+y1)//2))           # barra delle schede, lontano dalla scheda
sel = px(x0 + 8, y0 + 8)                             # dentro la scheda, fuori dal testo
corner = 7*scale                                     # raggio 6 + margine: solo i lati dritti
samples = []
for x in range(x0 + corner, x1 - corner):
    samples.append([px(x, y) for y in range(y0 - band, y0)])
    samples.append([px(x, y) for y in range(y1, y1 + band)])
for y in range(y0 + corner, y1 - corner):
    samples.append([px(x, y) for x in range(x0 - band, x0)])
    samples.append([px(x, y) for x in range(x1, x1 + band)])
for name, g in (('panel', panel), ('sel', sel)):
    best = [max(ratio(p, g) for p in s) for s in samples]
    best.sort()
    print(f"vs {name} {tuple(g)} n {len(best)} min {best[0]:.2f} p10 {best[len(best)//10]:.2f} mediana {statistics.median(best):.2f} quota>=3 {100*sum(b>=3 for b in best)/len(best):.0f}%")
