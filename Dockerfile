FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    git \
    && rm -rf /var/lib/apt/lists/*

COPY dist/snif-linux-x86_64 /usr/local/bin/snif
RUN chmod +x /usr/local/bin/snif

WORKDIR /workspace

ENTRYPOINT ["snif"]
