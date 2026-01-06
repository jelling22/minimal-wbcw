```
docker build -t minimal .
```
```
docker run -ti -e DISPLAY=host.docker.internal:0 -v ~/.Xauthority:/root/.Xauthority -v $(pwd):/opt/work --workdir /opt/work --entrypoint=/bin/bash minimal:latest
```
```
GST_DEBUG_DUMP_DOT_DIR=. cargo run --bin cef-cli
```
![dot graph](https://github.com/jelling22/minimal-wbcw/blob/main/minimal_playing.png)