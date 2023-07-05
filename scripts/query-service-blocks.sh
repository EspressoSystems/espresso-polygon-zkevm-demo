#!/usr/bin/env bash

HEIGHT=""

while true; do
    h=$(curl -s http://localhost:50001/status/latest_block_height)
    if [[ "$h" != "$HEIGHT" ]]; then
        HEIGHT="$h"
        QUERY_HEIGHT=$((HEIGHT - 1))
        tx="$(curl -s http://localhost:50001/availability/transaction/$QUERY_HEIGHT/0 | jq '.transaction' -c)"
        echo "Block: $QUERY_HEIGHT Transactions: $tx"
    fi
    sleep 1
done
