# Reproducible demo for the README recordings: diskgc and its dashboard in a
# container with a synthetic home directory, recorded with VHS.
#   docker build -f docs/demo/Dockerfile -t diskgc-demo .
#   docker run --rm -v "$PWD/docs/assets:/vhs/out" diskgc-demo dashboard.tape
#   docker run --rm -v "$PWD/docs/assets:/vhs/out" diskgc-demo plan.tape

FROM rust:1-bookworm AS core
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p diskgc

FROM dart:stable AS ui
RUN apt-get update && apt-get install -y --no-install-recommends git && rm -rf /var/lib/apt/lists/*
WORKDIR /src/ui
COPY ui/ .
RUN dart pub get && dart compile exe bin/diskgc_ui.dart -o /diskgc-ui

FROM ghcr.io/charmbracelet/vhs:v0.10.0
COPY --from=core /src/target/release/diskgc /usr/local/bin/diskgc
COPY --from=ui /diskgc-ui /usr/local/bin/diskgc-ui
COPY docs/demo/setup.sh docs/demo/history.py /usr/local/share/diskgc-demo/
COPY docs/demo/*.tape /vhs/
RUN chmod +x /usr/local/share/diskgc-demo/setup.sh
ENV HOME=/home/dev USER=dev TERM=xterm-256color
WORKDIR /vhs
