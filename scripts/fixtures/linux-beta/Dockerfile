# Dependency preparation only: building this image requires network access.
# Fixed base-image identities; amd64 can be emulated on an ARM Docker host.
FROM --platform=linux/amd64 node@sha256:4a4884e8a44826194dff92ba316264f392056cbe243dcc9fd3551e71cea02b90 AS node
FROM --platform=linux/amd64 rust@sha256:5b9332190bb3b9ece73b810cd1f1e9f06343b294ce184bcb067f0747d7d333ea
COPY --from=node /usr/local/bin/ /usr/local/bin/
COPY --from=node /usr/local/lib/node_modules/ /usr/local/lib/node_modules/
RUN apt-get update && apt-get install -y --no-install-recommends python3 ca-certificates && rm -rf /var/lib/apt/lists/*
# Pi's published npm-shrinkwrap pins its dependency graph. No TinyMem publishing,
# postinstall hooks, user credentials or source checkout are used here.
RUN mkdir -p /opt/pi && npm install --prefix /opt/pi --ignore-scripts --no-audit --no-fund --save-exact @earendil-works/pi-coding-agent@1.0.0
ENV PATH=/opt/pi/node_modules/.bin:$PATH \
    PI_OFFLINE=1 PI_SKIP_VERSION_CHECK=1 PI_TELEMETRY=0 \
    TINYMEM_PI_BIN=/opt/pi/node_modules/.bin/pi \
    TINYMEM_PI_MODULE=/opt/pi/node_modules/@earendil-works/pi-coding-agent/dist/index.js
WORKDIR /work
