FROM ubuntu:25.10

RUN apt-get update
RUN DEBIAN_FRONTEND=noninteractive apt-get -y install \
    libgstreamer1.0-dev \
    libgstreamer-plugins-base1.0-dev \
    libgstreamer-plugins-bad1.0-dev \
    gstreamer1.0-plugins-base \
    gstreamer1.0-plugins-good \
    gstreamer1.0-plugins-bad \
    gstreamer1.0-plugins-ugly \
    gstreamer1.0-libav \
    gstreamer1.0-tools \
    gstreamer1.0-alsa \
    gstreamer1.0-gl \
    gstreamer1.0-pulseaudio \
    git \
    build-essential \
    cmake \
    libssl-dev \
    libcurl4-openssl-dev \
    python3-pip \
    ninja-build \
    curl \
    rustup \
    graphviz

# Running outside of venv requires flag
RUN pip3 install meson --break-system-package

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain 1.89.0 -y

WORKDIR /opt

RUN mkdir -p crc32c && \
    cd crc32c && \
    curl -sSL https://github.com/google/crc32c/archive/1.1.2.tar.gz | \
    tar -xzf - --strip-components=1 && \
    cd /opt/crc32c && \
    cmake -S . -B build \
    -GNinja \
    -DCMAKE_INSTALL_PREFIX:PATH=/usr/local \
    -DCMAKE_INSTALL_LIBDIR:PATH=lib \
    -DBUILD_SHARED_LIBS=YES \
    -DCRC32C_USE_GLOG=NO \
    -DCRC32C_BUILD_TESTS=NO \
    -DCRC32C_BUILD_BENCHMARKS=NO && \
    cmake --build build --target install && \
    cd ../ && \
    rm -rf crc32c

RUN mkdir -p abseil-cpp && \
    cd abseil-cpp && \
    curl -sSL https://github.com/abseil/abseil-cpp/archive/20220623.2.tar.gz | \
    tar -xzf - --strip-components=1 && \
    sed -i 's/^#define ABSL_OPTION_USE_\(.*\) 2/#define ABSL_OPTION_USE_\1 0/' "absl/base/options.h"  && \
    sed -i '24i #include <cstdint>' absl/container/internal/container_memory.h && \
    cmake -S . -B build \
    -GNinja \
    -DBUILD_TESTING=NO \
    -DCMAKE_INSTALL_PREFIX:PATH=/usr/local \
    -DCMAKE_INSTALL_LIBDIR:PATH=lib \
    -DBUILD_SHARED_LIBS=YES  && \
    cmake --build build --target install && \
    cd ../ && \
    rm -rf abseil-cpp

RUN mkdir -p json && \
    cd json && \
    curl -sSL https://github.com/nlohmann/json/archive/v3.10.4.tar.gz | \
    tar -xzf - --strip-components=1 && \
    cmake \
    -DCMAKE_BUILD_TYPE=Release \
    -DBUILD_SHARED_LIBS=yes \
    -DJSON_BuildTests=OFF \
    -H. -Bcmake-out/nlohmann/json && \
    cmake --build cmake-out/nlohmann/json --target install -- -j $(cat /proc/cpuinfo | grep processor | wc -l) && \
    cd ../ && \
    rm -rf json

RUN mkdir -p google-cloud-cpp && \
    cd google-cloud-cpp && \
    curl -sSL https://github.com/googleapis/google-cloud-cpp/archive/v2.6.0.tar.gz | \
    tar --strip-components=1 -zxf - && \
    cmake -S . -B build \
    -GNinja \
    -DCMAKE_BUILD_TYPE=Debug \
    -DCMAKE_CXX_STANDARD=14 \
    -DCMAKE_INSTALL_PREFIX:PATH=/usr/local \
    -DCMAKE_INSTALL_LIBDIR:PATH=lib \
    -DBUILD_SHARED_LIBS=YES \
    -DBUILD_TESTING=NO \
    -DGOOGLE_CLOUD_CPP_ENABLE_WERROR=OFF \
    -DGOOGLE_CLOUD_CPP_ENABLE=storage && \
    cmake --build build --target install -- -v && \
    cd ../ && \
    rm -rf google-cloud-cpp

RUN mkdir -p gst-plugins-bad && \
    cd gst-plugins-bad  && \
    curl -sSL https://gstreamer.freedesktop.org/src/gst-plugins-bad/gst-plugins-bad-$(gst-inspect-1.0 --gst-version | awk '{print $5}').tar.xz | \
    tar --strip-components=1 -Jxf - && \
    meson setup -Dauto_features=disabled -Dgs=enabled builddir && \
    cd builddir && \
    ninja && \
    ninja install && \
    cd ../../ && \
    rm -rf gst-plugins-bad && \
    ldconfig

ENV GST_CEFSRC_SHA=b63340852fc93b0ab67b07200e1ff44f59ba6769
RUN git clone https://github.com/centricular/gstcefsrc.git && \
    cd gstcefsrc && \
    mkdir build && \
    git checkout $GST_CEFSRC_SHA && \
    rm -rf .git && \
    cd /opt/gstcefsrc/build && \
    cmake -G "Unix Makefiles" -DCMAKE_BUILD_TYPE=Release .. && \
    make && \
    make install && \
    cd ../../ && \
    rm -rf gstcefsrc

ENV GST_PLUGIN_PATH=/usr/local:$GST_PLUGIN_PATH
# Need the LD_PRELOAD to handle the TLS block issue
ENV LD_PRELOAD=/usr/local/libcef.so
ENV GST_CEF_CHROME_EXTRA_FLAGS=no-sandbox,disable-component-update,enable-logging=stderr
