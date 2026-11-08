# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# The update server (qs400-updated) as a container. Build from the
# repository root:
#
#   docker build -f ptf/updated/Dockerfile -t qs400-updated .
#   docker run -d -p 8080:8080 -v qs400-ptfs:/srv/qs400 qs400-updated
#
# It mirrors the ptf-catalog release into /srv/qs400 every 15 minutes
# and serves http://HOST:8080/qs400/v1/...; put TLS in front of it (or
# mount a certificate and pass --tls-cert and --tls-key). Arguments after
# the image name replace CMD (qs400-updated --help lists them).

FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
# rust-toolchain.toml pins the toolchain and the musl target; musl links
# with the toolchain's own rust-lld, host build scripts with clang
# (.cargo/config.toml, as on the build host).
RUN apt-get update && apt-get install -y --no-install-recommends clang \
 && rm -rf /var/lib/apt/lists/*
RUN rustup show active-toolchain || rustup toolchain install
RUN cargo build --release --locked -p ptf-updated --target x86_64-unknown-linux-musl \
 && mkdir -p /out/srv/qs400 \
 && cp target/x86_64-unknown-linux-musl/release/qs400-updated /out/qs400-updated

# A static binary with its own root certificates: nothing else is needed.
FROM scratch
COPY --from=build /out/qs400-updated /qs400-updated
COPY --from=build --chown=65534:65534 /out/srv/qs400 /srv/qs400
USER 65534:65534
VOLUME /srv/qs400
EXPOSE 8080
ENTRYPOINT ["/qs400-updated"]
CMD ["--listen", "0.0.0.0:8080", "--dir", "/srv/qs400"]
