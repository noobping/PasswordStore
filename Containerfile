FROM registry.fedoraproject.org/fedora:43

RUN dnf -y update && dnf -y install \
    bash \
    capnproto \
    cairo-devel \
    cargo \
    clang \
    clippy \
    curl \
    findutils \
    gcc \
    gcc-c++ \
    glib2-devel \
    glibc-langpack-en \
    gpgme-devel \
    gtk4-devel \
    hostname \
    libadwaita-devel \
    libgpg-error-devel \
    mingw64-binutils \
    mingw64-cairo \
    mingw64-gdk-pixbuf \
    mingw64-gcc \
    mingw64-gcc-c++ \
    mingw64-glib2 \
    mingw64-graphene \
    mold \
    mingw64-gtk4 \
    mingw64-libepoxy \
    mingw64-libgpg-error \
    mingw64-nettle \
    mingw64-openssl \
    mingw64-pango \
    nettle-devel \
    openssl-devel \
    pango-devel \
    pkg-config \
    pkgconf-pkg-config \
    ripgrep \
    rustfmt \
    shadow-utils \
    sudo \
    which \
    && dnf clean all

RUN useradd -m -G wheel -s /bin/bash nick \
    && echo '%wheel ALL=(ALL) NOPASSWD: ALL' > /etc/sudoers.d/10-wheel-nopasswd \
    && chmod 440 /etc/sudoers.d/10-wheel-nopasswd

USER nick
WORKDIR /home/nick

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

ENV PATH="/home/nick/.cargo/bin:${PATH}"
ENV PKG_CONFIG="x86_64-w64-mingw32-pkg-config"
ENV PKG_CONFIG_ALLOW_CROSS="1"

RUN rustup target add x86_64-pc-windows-gnu

CMD ["/bin/bash"]
