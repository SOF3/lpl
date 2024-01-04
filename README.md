# lpl

A command-line utility to plot charts from line-based inputs.

[![](https://sof3.github.io/lpl/sinusoidal.gif)](examples/sinusoidal.tape)

## Features

- Multiple data sources in different formats as polling or streaming inputs:
  - [x] JSON/JSONLines
  - [ ] CSV
- Interactive scrolling
- Series hiding/color selection

## Example usage

Plot new entries in `file.csv`,
number of instances for each replicaset
and current battery level of BAT0:

```sh
lpl \
  --csv <(tail -n+1 -f file.csv) \
  --json <(kubectl get replicaset -ojson -w | jq --unbuffered -c '{(.metadata.name): .status.replicas}') \
  --csv-watch battery=/sys/class/power_supply/BAT0/capacity
```

## Installation

Download development builds:

- Windows:
  [x86\_64 (64-bit)](https://sof3.github.io/lpl/bin-x86_64-pc-windows-msvc/lpl.exe),
  [aarch64 (ARM 64-bit)](https://sof3.github.io/lpl/bin-aarch64-pc-windows-msvc/lpl.exe),
  [i686 (Intel/AMD 32-bit)](https://sof3.github.io/lpl/bin-i686-pc-windows-msvc/lpl.exe)
- MacOS:
  [x86\_64 (Intel/AMD 64-bit)](https://sof3.github.io/lpl/bin-x86_64-apple-darwin/lpl)
  [aarch64 (Silicon 64-bit)](https://sof3.github.io/lpl/bin-aarch64-apple-darwin/lpl)
- Linux:
  - x86\_64 (Intel/AMD 64-bit) [GNU](https://sof3.github.io/lpl/bin-x86_64-unknown-linux-gnu/lpl),
    [musl](https://sof3.github.io/lpl/bin-x86_64-unknown-linux-musl/lpl)
  - aarch64 (ARM 64-bit) [GNU](https://sof3.github.io/lpl/bin-aarch64-unknown-linux-gnu/lpl),
    [musl](https://sof3.github.io/lpl/bin-aarch64-unknown-linux-musl/lpl)
  - i686 (Intel/AMD 32-bit) [GNU](https://sof3.github.io/lpl/bin-aarch64-unknown-linux-gnu/lpl),
    [musl](https://sof3.github.io/lpl/bin-aarch64-unknown-linux-musl/lpl)
