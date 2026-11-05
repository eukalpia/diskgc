# Demo recordings

The GIFs and screenshots in the README are recorded with [VHS](https://github.com/charmbracelet/vhs)
inside a container with a synthetic home directory (`setup.sh`): stale Rust targets,
`node_modules` of an abandoned branch, Flutter build output, agent scratch directories and old app
versions, all allocated for real, plus two weeks of generated history (`history.py`). Nothing
from a real machine appears in them.

```sh
docker build -f docs/demo/Dockerfile -t diskgc-demo .
docker run --rm -v "$PWD/docs/assets:/vhs/out" diskgc-demo dashboard.tape
docker run --rm -v "$PWD/docs/assets:/vhs/out" diskgc-demo plan.tape
```

The demo allocates about 4 GB inside the container; it is gone with the container.
