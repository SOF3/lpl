# lpl

A command-line utility to plot charts from line-based inputs.

[![](https://sof3.github.io/lpl/sinusoidal.gif)](examples/sinusoidal.tape)

## Example usage

Plot new entries in `file.csv`,
number of instances for each replicaset
and current battery level of BAT0:

```sh
lpl \
  --csv <(tail -n+1 -f file.csv) \
  --json <(kubectl get replicaset -ojson -w | jq --unbuffered -c '{(.metadata.name): .status.replicas}') \
  --single-watch battery=/sys/class/power_supply/BAT0/capacity
```

## Installation

Download development builds:

- Windows:
  [x86\_64 (64-bit)](https://sof3.github.io/lpl/bin-x86_64-pc-windows-msvc/lpl.exe),
  [32-bit](https://sof3.github.io/lpl/bin-i686-pc-windows-msvc/lpl.exe)
- MacOS:
  [x86\_64](https://sof3.github.io/lpl/bin-x86_64-apple-darwin/lpl)
- Linux:
  [x86\_64 GNU](https://sof3.github.io/lpl/bin-x86_64-unknown-linux-gnu/lpl),
  [x86\_64 musl](https://sof3.github.io/lpl/bin-x86_64-unknown-linux-musl/lpl),
