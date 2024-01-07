# lpl

A command-line utility to plot charts from line-based inputs.

[![Sinusoidal plot example](https://sof3.github.io/lpl/sinusoidal.gif)](examples/sinusoidal.tape)

## Features

- Multiple data sources in different formats as polling or streaming inputs:
  - [x] JSON (inotify + periodic reload)
  - [x] JSONLines (streaming input)
  - [x] CSV (inotify + periodic reload)
  - [x] CSV (streaming input)
- Interactive scrolling
- Series hiding/color selection

## Example usage

A few simple use cases using Bash:

### Device thermometers

Linux exposes the device thermometers under /sys/class/thermal.

For example, on my laptop:

```console
$ cat /sys/class/thermal/*/temp
47000
20000
39050
45000
47050
40000
47000
```

Therefore, we can pass each file as a separate `--csv-poll` argument to `lpl`:

```sh
$ lpl $(
>     for file in /sys/class/thermal/*/temp; do
>          echo "--csv-poll $(basename $(dirname $file))=$file"
>     done
> )
```

### Rolling deployment

When performing a [rolling update][rolling update]
with a Kubernetes Deployment,
it scales down the old ReplicaSet and scales up the new ReplicaSet gradually.
We can monitor this progress by watching the ReplicaSet replica count
using `<()` process substitution with `kubectl get --watch`
and some transformation with `jq`:

```sh
lpl --json <(kubectl get replicaset --watch -ojson | \
        jq --unbuffered -c '{
            (.metadata.name + " total"): .status.replicas,
            (.metadata.name + " ready"): .status.readyReplicas,
        }')
```

Currently `lpl` only supports JSONLines,
so make sure to pass `-c` to print one JSON object per line.
Also use `--unbuffered` to ensure `jq` can emit new events immediately.

## Reference

### Input sources

#### JSON

JSON comes in two modes &mdash; streaming and polling.

Streaming JSON is specified by `--json PATH`,
where `PATH` is a special file (typically from `<()` process substitution)
that reads one JSON object per line.
A JSON object may contain arbitrary fields,
but **only top-level fields with a single numeric value are processed**,
where the numeric value is added to the time series
named with the corresponding key,
at the time the line is read.
Pipe your input to `| jq -c --unbuffered .` if it was not already one line per object.

Polling JSON, on the other hand, is a (usually regular) file
that contains a single JSON object (either compact or formatted),
specified by `--json-poll PATH`.
Similar to streaming JSON,
only top-level fields with a single numeric value are processed.
The file is reloaded when it is chnaged (where supported by inotify),
or every `--poll-period` seconds.

#### CSV

Similar to JSON, CSV also supports streaming and polling modes
through `--csv PATH` and `--csv-poll [HEADER=]PATH`.

CSV files are separated with `,` by default,
but this may be customized with `--csv-poll-delimiter`.

If the argument contains `=`,
the part before `=` is treated as the CSV header.
Otherwise, the first line read from `PATH`
(initial first line for streaming, first line from every reload for polling)
is treated as the CSV header.

Only numeric values that can be [parsed as `f64`][f64 as FromStr] are processed;
other values are silently ignored.
For polling mode, if there are multiple files in a single poll,
only the first numeric value is processed.

### Interactive CLI

`lpl` provides an interactive TUI to browse the data plot.

Type `?` for help.
Type `q` to quit.

## Installation

### Compile from source

Use `cargo install`:

```sh
cargo install lpl
```

### Development builds

Download development builds built on GitHub CI:

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

  [rolling update]: https://kubernetes.io/docs/concepts/workloads/controllers/deployment/#rolling-update-deployment
  [f64 as FromStr]: https://doc.rust-lang.org/std/primitive.f64.html#impl-FromStr-for-f64
