# lpl

A command-line utility to plot charts from line-based inputs.

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
