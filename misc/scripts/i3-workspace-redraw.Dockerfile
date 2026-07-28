# docker build -f misc/scripts/i3-workspace-redraw.Dockerfile -t rio-i3-e2e .
# docker run --rm rio-i3-e2e
# Add "-v $PWD/e2e-artifacts:/artifacts" and "--artifacts /artifacts" to keep
# screenshots and Rio logs after the container exits.

FROM rust:1-bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        glslang-tools \
        libasound2-dev \
        libfontconfig1-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .
RUN cargo build -p rioterm --no-default-features --features x11

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        dbus-x11 \
        gawk \
        i3-wm \
        imagemagick \
        jq \
        libasound2 \
        libfontconfig1 \
        libgl1-mesa-dri \
        libxi6 \
        mesa-vulkan-drivers \
        tini \
        xauth \
        xvfb \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/debug/rio /usr/local/bin/rio
COPY misc/scripts/test-i3-workspace-redraw.sh \
    misc/scripts/test-i3-workspace-redraw-headless.sh \
    /usr/local/libexec/rio/

RUN chmod +x \
    /usr/local/libexec/rio/test-i3-workspace-redraw.sh \
    /usr/local/libexec/rio/test-i3-workspace-redraw-headless.sh

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/libexec/rio/test-i3-workspace-redraw-headless.sh"]
CMD ["--candidate", "/usr/local/bin/rio"]
