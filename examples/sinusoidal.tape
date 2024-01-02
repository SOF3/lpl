Output "target/sinusoidal.gif"

Require jq

Set Shell "bash"
Set FontSize 12
Set Width 1200
Set Height 800

Type@1ms `lpl --data-backlog-duration 10 --json <(for i in {1..1000}; do jq -nc --arg i $i '$i | tonumber * 0.08 | {sin: sin, "0.5 cos": (cos * 0.5)}'; sleep 0.02; done)`
Sleep 1s
Enter
Sleep 100ms

Sleep 3s
Type@100ms "lll"
Sleep 2s
Type "h"
Sleep 2s
Type "gjj"
Type@500ms "cccccc"
Sleep 1s
