#!/usr/bin/env bash
# Usage:
# If no demo is running, the local demo can be started with
#
#   env RUST_LOG_FORMAT=full just demo
#
session="demo"

tmux kill-session -t $session
tmux new-session -d -s $session

window=0
tmux rename-window -t $session:$window 'Espresso Polygon ZkEVM demo'

tmux select-pane -T "Sequencing and HotShot Query Service"
tmux send-keys -t $session:$window 'docker logs -f demo-sequencer0-1' C-m

tmux split-window -v -t $session:$window
tmux select-pane -T "HotShot Commitment"
tmux send-keys -t $session:$window 'docker logs -f demo-sequencer1-1 2>&1 | grep hotshot_commitment' C-m

tmux split-window -v -t $session:$window
tmux select-pane -T "zkevm-node"
tmux send-keys -t $session:$window 'docker logs -f demo-zkevm-permissionless-node-1 2>&1 | grep "processBatchRequest.BatchL2Data"' C-m

tmux split-window -v -t $session:$window
tmux select-pane -T "zkevm-prover"
tmux send-keys -t $session:$window 'docker logs -f demo-zkevm-prover-1 2>&1 | grep PROVER_PROCESS_BATCH' C-m

tmux select-layout even-vertical
tmux set -t $session:$window pane-border-status top

tmux attach-session -t $session
