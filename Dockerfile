FROM archlinux

RUN pacman -Syu --noconfirm base-devel wget git grub libisoburn python mtools && useradd builduser -m && passwd -d builduser && printf 'builduser ALL=(ALL) ALL\n' | tee -a /etc/sudoers && sudo -u builduser bash -c 'cd ~ && git clone https://aur.archlinux.org/automake-1.15.git && cd automake-1.15 && makepkg -si --noconfirm'

COPY . /seraph

RUN cd /seraph && ./toolchain.sh && rm -rf bin isodir kernel lib libc linker ports script sysroot toolchain/tarballs toolchain/gcc-build toolchain/gcc-build-2 toolchain/binutils-build toolchain/patches headers.sh toolchain.sh

RUN pacman -R --noconfirm bison fakeroot file groff gcc binutils patch pkgconf sed sudo wget && pacman -Sc --noconfirm

ENV PATH="/seraph/toolchain/bin:${PATH}"
