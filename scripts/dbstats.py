#!/usr/bin/env python3
"""Print face and stylised-face counts for a pichouse library.db.

The script opens the database read-only.
Usage: python3 scripts/dbstats.py [path/to/library.db]
The default path is ~/.local/share/pichouse/library.db.
"""
import os
import sqlite3
import sys
import time

p = sys.argv[1] if len(sys.argv) > 1 else os.path.expanduser(
    '~/.local/share/pichouse/library.db')
c = sqlite3.connect(f'file:{p}?mode=ro', uri=True)

UNNAMED_FACE = 'ignored=0 and (person_id is null or person_id=0)'
UNNAMED_STYLE = 'ignored=0 and (character_id is null or character_id=0)'
QUERIES = [
    ('photos', 'select count(*) from photos'),
    ('faces total', 'select count(*) from faces'),
    ('faces usable', 'select count(*) from faces where ignored=0 and embedding is not null'),
    ('faces with person', 'select count(*) from faces where ignored=0 and person_id is not null and person_id<>0'),
    ('faces ignored', 'select count(*) from faces where ignored=1'),
    ('persons', 'select count(*) from persons'),
    ('face groups (unnamed)', f'select count(distinct cluster_id) from faces where {UNNAMED_FACE}'),
    ('face singletons', f'select count(*) from (select cluster_id from faces where {UNNAMED_FACE} group by cluster_id having count(*)=1)'),
    ('style faces total', 'select count(*) from style_faces'),
    ('style faces usable', 'select count(*) from style_faces where ignored=0 and embedding is not null'),
    ('style with character', 'select count(*) from style_faces where ignored=0 and character_id is not null and character_id<>0'),
    ('style noise (-1)', f'select count(*) from style_faces where {UNNAMED_STYLE} and cluster_id=-1'),
    ('style ignored', 'select count(*) from style_faces where ignored=1'),
    ('characters', 'select count(*) from characters'),
    ('style groups (unnamed)', f'select count(distinct cluster_id) from style_faces where {UNNAMED_STYLE} and cluster_id<>-1'),
    ('face emb bytes', 'select length(embedding) from faces where embedding is not null limit 1'),
    ('style emb bytes', 'select length(embedding) from style_faces where embedding is not null limit 1'),
]

for name, sql in QUERIES:
    t = time.time()
    try:
        v = c.execute(sql).fetchone()[0]
    except Exception as e:  # noqa: BLE001
        v = f'ERR {e}'
    print(f'{name:24} {v}   ({time.time() - t:.1f}s)')
print('db size GB', round(os.path.getsize(p) / 1e9, 2))
