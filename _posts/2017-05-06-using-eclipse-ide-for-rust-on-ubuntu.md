---
title: "Using Eclipse IDE for Rust on Ubuntu"
layout: post
---

In [my first blog post](
{{ site.baseurl }}{% post_url 2017-04-30-getting-started-with-rust %}) I was
starting out with Atom editor but quickly realized that it is lacking features
of Integrated Development Environments (IDEs). I need to be able to click on
functions and data types to jump to their definitions and was not able to get
that working in Atom.

Looking further at [areweideyet.com](https://areweideyet.com/) I found
[Eclipse](https://eclipse.org/) listed, an IDE I have used a long time ago for
Java development. Let's give it a try!

## Installing Java to run Eclipse

```
sudo apt install openjdk-8-jre
```

In my first installation attempt Eclipse had problems to connect to external
Github software sources sites because of missing Java SSL root certificates.
After [filing an issue in the wrong
place](https://github.com/RustDT/RustDT/issues/162) I found the following [fix
for missing Java SSL certificates](http://stackoverflow.com/a/33440168/2000435):

```
sudo dpkg --purge --force-depends ca-certificates-java
sudo apt-get install ca-certificates-java
```

## Installing Eclipse base

The [Eclipse downloads page](https://www.eclipse.org/downloads/) gives you an
installer where you select additional packages for whatever platform you want
to develop on. You can skip all of that because we will install the external
Rust extension later, so we select "Eclipse Platform (Neon)".

On the "Projects" step of the installer I only selected EGit for Git support in
Eclipse which I always need. As installation folder I picked "eclipse" which
means Eclipse will end up in your home directory for example at
```/home/klausi/eclipse```. When you finish the installation you will have to
accept some license agreements and trust some certificates.

In the end Eclipse should start up successfully.

## Adding a launcher shortcut

Create a new file at ```~/.local/share/applications/eclipse.desktop``` with
content like this:

```
[Desktop Entry]
Name=Eclipse
Type=Application
Exec=/home/klausi/eclipse/eclipse/eclipse
Terminal=false
Icon=/home/klausi/eclipse/eclipse/icon.xpm
Comment=Integrated Development Environment
NoDisplay=false
Categories=Development;IDE;
Name[en]=Eclipse
```

## Installing RustDT on Ubuntu

Make sure you have a Rust toolchain environment set up:

```
curl https://sh.rustup.rs -sSf | sh
```
(the usual security warning: make sure to trust your sources before you execute
 random scripts from the internet.)

```
source $HOME/.cargo/env
cargo install racer
cargo install rustfmt
cargo install --git https://github.com/RustDT/Rainicorn --tag version_1.x
rustup component add rust-src
```

Now we need the RustDT Eclipse plugin from
[rustdt.github.io](https://rustdt.github.io/). Follow the installation and
configuration instructions there closely.

My settings in Window -> Preferences -> Rust:

* Rust installation directory: ```/home/klausi/.cargo```
* Rust "src" directory:
```/home/klausi/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/src```
* Racer: ```/home/klausi/.cargo/bin/racer```
* Rainicorn: ```/home/klausi/.cargo/bin/parse_describe```
* rustfmt: ```/home/klausi/.cargo/bin/rustfmt```

I'm also using the option "Format automatically on editor save". No questions
anymore what the correct code style is :-)

## Conclusion

The installation of Eclipse is a bit tedious and RustDT takes a bit of
configuration, but I think it is totally worth it to be more productive when
writing Rust. Eclipse works better for me than Atom Editor. I also filed a
[pull request to update the available Eclipse features on
areweideyet.com](https://github.com/contradictioned/areweideyet/pull/46).
