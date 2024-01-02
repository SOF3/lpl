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

- Windows: [x86_64](https://sof3.github.io,lpl,lpl-windows/lpl.exe)
- MacOS: [x86_64](https://sof3.github.io,lpl,lpl-macos/lpl)
- Linux: [x86_64](https://sof3.github.io,lpl,lpl-linux/lpl)
