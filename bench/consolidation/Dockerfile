ARG BASE_REFERENCE=xsh-comparative-base:unprepared
FROM ${BASE_REFERENCE}
ARG BASE_IMAGE
ARG TARGETARCH
LABEL org.xsh.comparative.base=${BASE_IMAGE}
COPY dash.tar.gz oils.tar.gz nu.tar.gz elvish.tar.gz versions.lock.json /opt/comparators/source/
COPY checksums /opt/comparators/source/checksums
RUN cd /opt/comparators/source && sha256sum -c checksums
RUN mkdir -p /opt/comparators/bin /opt/comparators/build \
    && cd /opt/comparators/build \
    && tar xzf ../source/dash.tar.gz \
    && cd dash-0.5.12 \
    && CC=/usr/bin/clang-23 CFLAGS="-O2" ./configure --prefix=/opt/comparators \
    && make -j2 \
    && make install
RUN cd /opt/comparators/build \
    && tar xzf ../source/oils.tar.gz \
    && cd oils-for-unix-0.37.0 \
    && ./configure --cxx-for-configure /usr/bin/clang-23 --without-readline --prefix /opt/comparators \
    && CXXFLAGS='-include algorithm -include functional -include type_traits' OILS_PARALLEL_BUILD=0 _build/oils.sh --cxx clang++ --variant opt --without-readline
RUN cd /opt/comparators/build/oils-for-unix-0.37.0 \
    && ./install _bin/clang++-opt-sh/oils-for-unix.stripped
RUN cd /opt/comparators/build \
    && mkdir nu elvish \
    && tar xzf ../source/nu.tar.gz -C nu --strip-components=1 \
    && tar xzf ../source/elvish.tar.gz -C elvish \
    && install -m 0755 nu/nu /opt/comparators/bin/nu \
    && install -m 0755 elvish/elvish /opt/comparators/bin/elvish \
    && /opt/comparators/bin/nu --version \
    && /opt/comparators/bin/elvish -version \
    && /opt/comparators/bin/ysh --version \
    && rm -rf /opt/comparators/build
