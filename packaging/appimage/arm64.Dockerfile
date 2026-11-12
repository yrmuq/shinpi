FROM rust:1-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libfontconfig1-dev libxkbcommon-dev libwayland-dev libgl1-mesa-dev \
    libegl1-mesa-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev file zsync curl \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    sh packaging/appimage/build.sh && sh packaging/appimage/check.sh
FROM scratch
COPY --from=build /src/dist/ /
