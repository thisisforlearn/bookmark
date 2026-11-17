# models/

Place `.bkmdl` model files here (or in your configured `models_dir`):

- `spell.bkmdl` — typo re-ranker
- `embed.bkmdl` — embedding encoder
- `rank.bkmdl`  — LTR re-ranker

Generate them with:

```sh
bookmark model train all --mode quick
```

then copy the files here, or train directly into your data directory (the default). The server
works without any of these files — search falls back to lexical + SymSpell and the UI shows a
"degraded" badge.
