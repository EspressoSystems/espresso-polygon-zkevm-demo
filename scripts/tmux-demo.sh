#!/usr/bin/env bash
# Usage:
# If no demo is running, the local demo can be started with
#
#   env RUST_LOG_FORMAT=full just demo
#
session="espresso-polygon-zkevm-demo"

tmux kill-session -t $session
tmux new-session -d -s $session

window=0
tmux rename-window -t $session:$window 'Espresso Polygon ZkEVM demo'

tmux select-pane -T "Sequencer node logs"
tmux send-keys -t $session:$window 'docker logs -f demo-sequencer0-1' C-m
tmux select-layout even-vertical # ensure enough space

tmux split-window -v -t $session:$window
tmux select-pane -T "Query Service API"
tmux send-keys -t $session:$window './scripts/query-service-blocks.sh' C-m
tmux select-layout even-vertical # ensure enough space

tmux split-window -v -t $session:$window
tmux select-pane -T "HotShot Commitment Task"
tmux send-keys -t $session:$window 'docker logs -f demo-sequencer1-1 2>&1 | grep hotshot_commitment' C-m
tmux select-layout even-vertical # ensure enough space

tmux split-window -v -t $session:$window
tmux select-pane -T "Polygon zkEVM node"
tmux send-keys -t $session:$window 'docker logs -f demo-zkevm-permissionless-node-1 2>&1 | grep "processBatch\[processBatchRequest.BatchL2Data\]"' C-m
tmux select-layout even-vertical # ensure enough space

tmux split-window -v -t $session:$window
tmux select-pane -T "Polygon zkEVM prover"
tmux send-keys -t $session:$window 'docker logs -f demo-zkevm-prover-1 2>&1 | grep "gen_[a-z]*_proof"' C-m
tmux select-layout even-vertical # ensure enough space

tmux set -t $session:$window pane-border-status top

tmux attach-session -t $session
