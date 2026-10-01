"""Conta itens por conteúdo em cada perfil. Uso: python scripts/dbcount.py <prefixo> [perfis...]"""
import sqlite3, sys
prefix = sys.argv[1]
profiles = sys.argv[2:] or ["a", "b"]
for p in profiles:
    c = sqlite3.connect(rf"C:\Users\wwall\AppData\Roaming\app.clipsync.desktop\profiles\{p}\clipsync.db")
    rows = list(c.execute("select content, count(*), group_concat(source_device_id) from clipboard_items where content like ? group by content order by content", (prefix + "%",)))
    dups = [r[0] for r in rows if r[1] > 1]
    print(f"{p}: {len(rows)} distintos, dups={dups}")
    for r in rows:
        print("   ", r[0], "| fontes:", r[2][:16])
