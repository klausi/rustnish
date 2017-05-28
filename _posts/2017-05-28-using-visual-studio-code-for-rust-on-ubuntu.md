---
title: "Using Visual Studio Code for Rust on Ubuntu"
layout: post
---

I already wrote about [using Eclipse for Rust development]({{ site.baseurl }}{% post_url 2017-05-06-using-eclipse-ide-for-rust-on-ubuntu %}) but after trying [Visual Studio Code](https://code.visualstudio.com/) (VSCode) I think it is slightly better than the Eclipse integration:

* when the cursor is at a variable the same variable is highlighted elsewhere.
* tooltip popups when hovering over variables, functions, methods.
* better native support for Git and Markdown files

Syntax highlighting, autocompletion, Ctrl + Click on functions and auto-formatting of course also work in VSCode as you would expect from an IDE.

## Installing Rust on Ubuntu

First make sure you have a Rust toolchain environment set up:

```
curl https://sh.rustup.rs -sSf | sh
```
(the usual security warning: make sure to trust your sources before you execute
 random scripts from the internet.)

```
source $HOME/.cargo/env
cargo install racer
cargo install rustfmt
rustup component add rust-src
```

## Installing VSCode and extensions

Download the Deb package from [code.visualstudio.com](https://code.visualstudio.com) and install it with the Ubuntu software center or dpkg. You should now have a launcher for vscode and the ```code``` command to start the IDE.

Next install the following extensions:
* Rust code completion and auto formatting: [Rust](https://marketplace.visualstudio.com/items?itemName=kalitaalexey.vscode-rust)
* TOML configuration files syntax highlighting: [Better TOML](https://marketplace.visualstudio.com/items?itemName=bungcip.better-toml)
* LLDB debugging for Rust programs: [LLDB Debugger](https://marketplace.visualstudio.com/items?itemName=vadimcn.vscode-lldb)
* nice file icons: [vscode-icons](https://marketplace.visualstudio.com/items?itemName=robertohuertasm.vscode-icons)

The Rust extension has experimental support for [Rust Language Server](https://github.com/rust-lang-nursery/rls), but it does not work reliably yet. That's why you have to enable the racer legacy mode in the VSCode settings.

Go to File -> Preferences -> Settings and the editor will open your settings JSON file. Here are some very useful settings you should use:

```json
{
    "editor.formatOnSave": true,
    "editor.rulers": [
        80
    ],
    "files.trimTrailingWhitespace": true,
    "rust.actionOnSave": "check",
    "rust.forceLegacyMode": true,
    "workbench.iconTheme": "vscode-icons"
}
```

## Conclusion

Although VSCode has a sparse user interface (Back/Forward buttons are missing for example when navigating through code) it is a decent IDE for Rust development. It offers freely configurable keyboard shortcuts and a comprehensible settings editor. The Rust extension is a bit better than the one for Eclipse IDE.
