#!/usr/bin/env python3
"""Genera un Markdown realistico e misto (titoli, paragrafi, liste, codice, tabelle, citazioni).
Uso: gen_md.py OUT.md TARGET_BYTES [SEED]"""
import random, sys

WORDS = ("il la di che e un una per con non sono come più anche questo quando dove sempre "
 "sistema file dati utente memoria cache rendering pagina testo blocco riga colonna tabella "
 "servizio richiesta risposta errore stato valore chiave indice ricerca documento sezione "
 "performance latenza scroll frame finestra editor lettore markdown parser albero nodo "
 "the of and to in is that for with on as by this from are be at have it not an or which "
 "build release deploy branch commit review test bench layout glyph shader texture buffer").split()
LANGS = {
 "rust": ["fn {n}(input: &str) -> Result<usize, Error> {{", "    let parts: Vec<_> = input.split(',').collect();",
          "    if parts.is_empty() {{ return Err(Error::Empty); }}", "    Ok(parts.iter().map(|p| p.len()).sum())", "}}"],
 "python": ["def {n}(rows, limit=100):", "    \"\"\"Raggruppa le righe per chiave.\"\"\"", "    out = {{}}",
            "    for row in rows[:limit]:", "        out.setdefault(row['key'], []).append(row)", "    return out"],
 "js": ["export async function {n}(url, opts = {{}}) {{", "  const res = await fetch(url, {{ ...opts, method: 'GET' }});",
        "  if (!res.ok) throw new Error(`HTTP ${{res.status}}`);", "  return res.json();", "}}"],
 "sh": ["#!/bin/sh", "set -eu", "for f in \"$@\"; do", "  wc -c \"$f\"", "done | sort -n | tail -5"],
 "json": ["{{", "  \"name\": \"{n}\",", "  \"version\": \"1.4.2\",", "  \"deps\": {{ \"serde\": \"1\", \"tokio\": \"1.38\" }}", "}}"],
}

def words(r, n):
    return " ".join(r.choice(WORDS) for _ in range(n))

def sentence(r):
    s = words(r, r.randint(8, 22))
    roll = r.random()
    if roll < .15: s += f" **{words(r, 2)}**"
    elif roll < .3: s += f" `{r.choice(WORDS)}_{r.randint(1,99)}()`"
    elif roll < .4: s += f" [{words(r, 2)}](https://example.com/{r.choice(WORDS)})"
    elif roll < .5: s += f" *{words(r, 3)}*"
    return s[0].upper() + s[1:] + "."

def para(r): return " ".join(sentence(r) for _ in range(r.randint(2, 7)))

def block(r, i):
    k = r.random()
    if k < .08: return f"## {words(r, r.randint(2,5)).capitalize()} {i}"
    if k < .14: return f"### {words(r, r.randint(2,4)).capitalize()}"
    if k < .52: return para(r)
    if k < .64:
        mark = (lambda j: f"{j+1}.") if r.random() < .4 else (lambda j: "-")
        lines = []
        for j in range(r.randint(3, 7)):
            lines.append(f"{mark(j)} {sentence(r)}")
            if r.random() < .2: lines.append(f"    - {sentence(r)}")
        return "\n".join(lines)
    if k < .74:
        lang = r.choice(list(LANGS)); n = f"{r.choice(WORDS)}_{r.randint(1,999)}"
        body = [l.format(n=n) for l in LANGS[lang]] * r.randint(1, 3)
        return f"```{lang}\n" + "\n".join(body) + "\n```"
    if k < .82:
        cols = r.randint(3, 5)
        head = "| " + " | ".join(words(r, 1).capitalize() for _ in range(cols)) + " |"
        sep = "|" + "|".join(r.choice([" --- ", " :--: ", " --: "]) for _ in range(cols)) + "|"
        rows = ["| " + " | ".join(r.choice([words(r, r.randint(1,3)), str(r.randint(1, 99999)), f"`{r.choice(WORDS)}`"]) for _ in range(cols)) + " |" for _ in range(r.randint(3, 8))]
        return "\n".join([head, sep, *rows])
    if k < .90: return "> " + sentence(r) + "\n> " + sentence(r)
    if k < .93: return "---"
    return para(r)

def main():
    out, target = sys.argv[1], int(sys.argv[2])
    r = random.Random(int(sys.argv[3]) if len(sys.argv) > 3 else 42)
    parts, size, i = [f"# Documento di prova ({target // 1024} KB)\n"], 0, 0
    while size < target:
        b = block(r, i); parts.append(b); size += len(b.encode()) + 2; i += 1
    open(out, "w").write("\n\n".join(parts) + "\n")
    print(out, size, "byte,", i, "blocchi")

main()
