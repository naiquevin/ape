# ape

A _monkey-see-monkey-do_ approach to AI-assisted coding (or file
editing in general). Think of it as emacs's keyboard-macro-like
functionality but AI-driven.

## Motivation

I've sometimes felt the need to be able to "show" a code change to LLM
rather than having to explain it in plain English, which can get
tedious and is often prone to ambiguity. The LLM can then be asked to
make similar change elsewhere in the code base.

I built this tool as an experiment to see how well this approach works
in practice and whether it is cost-efficient (i.e., consumes fewer
tokens). My initial observation is that it kind of works (in the same
way that LLMs generally work!). I haven’t done any comparative
analysis of token usage yet.

## Installation

The Emacs mode is not on MELPA yet. It also depends on a CLI component
(distributed as part of the same repository). Follow these
instructions to install it manually:

Clone the repo

``` shell
git clone git@github.com:naiquevin/ape.git
```

There are three components:

1. `ape-cli`: A CLI tool written in Rust. It does most of the heavy
   lifting.
2. `ape-mode.el`: The emacs minor mode, a thin wrapper over the CLI
   for emacs integration
3. `ape-mcp-server`: Intended to provide the same functionality as the
   CLI but packaged as an MCP server for use with TUI-based coding
   agents. More on this later.

For emacs integration you only need `ape-cli` and `ape-mode.el`.

Build `ape-cli` and copy the binary to a directory in `PATH`

``` shell
cd ape
cargo build -p ape-cli --release
cp target/release/ape-cli ~/.local/bin
```

Copy the `elisp/ape-mode.el` file some where in your load-path and add
`(require 'ape-mode)` in your config. If you use `use-package` you can
instead add:

``` emacs-lisp
(use-package ape-mode
  :ensure nil
  :load-path "</path/to/ape-mode.el>"
  :config
  (ape-mode 1))
```

## Usage

The minor mode works as follows:

1. Start an ape macro recording with `C-c x (` (or `M-x
   ape-start-macro`)

2. Make changes to the file

3. Stop the macro recording with `C-c x )` (or `M-x ape-stop-macro`)

4. Ask the LLM to repeat the change with `C-c x e` (or `M-x
   ape-execute`). The diff returned by the LLM is displayed in a
   `diff-mode`-like buffer, from which you can accept or reject the
   change.

Other functions provided by the mode:

`ape-cancel-macro`: Cancels an ongoing macro recording

`ape-view-macro`: Shows a list of all recorded macros to the user. You
can view the diff for a particular macro and activate it (i.e. make it
the "currently active" macro that `ape-execute` will use).

## ape-mode configuration

By default, `ape-mode` assumes `ape-cli` executable to be in
`PATH`. Alternately, `ape-cli-command` custom variable can be set to
absolute path.

## LLM configuration

LLM related configuration is stored in `~/.ape/config.json` file. It
gets created with default values upon first use. The defaults are:

``` json
{
  "provider": "OpenAI",
  "model": "gpt-5-mini"
}
```

The default model is `gpt-5-mini` because I found it to be the most
accurate for rust and python code, often better than the more recent
models by OpenAI and Claude.

Credentials must be set as environment variables `OPENAI_API_KEY` and
`ANTHROPIC_API_KEY` (only these two providers are supported at
present).

When using emacs, if these variables are not present in emacs's
environment, it will prompt you for the relevant API key upon first
use.

## The MCP server

The MCP server implementation is not fully tested yet. The idea is to
expose the same functionality for TUI-based AI-coding tools.

However the current implementation relies of MCP sampling capability
in the client. Funnily enough, Claude itself led me to believe that
claude-code supports MCP server sampling when it [actually
doesn't](https://github.com/anthropics/claude-code/issues/1785).


























