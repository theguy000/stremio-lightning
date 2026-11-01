#!/bin/sh
# Keeps a worker's work in a commit even when the worker dies mid-step.
#
# Every two minutes, for every worktree under .scratch/wt/, commit whatever changed with a
# "[checkpoint]" message. Checkpoint commits are local only; a worker replaces them with real
# commits as it goes and the supervisor squashes them before a PR is opened.
#
# Started with `setsid nohup scripts/checkpoint-worktrees.sh &` so a harness restart does not
# kill it. Stop it by creating .scratch/tmp/checkpoints-off.
set -eu
root=$(git rev-parse --show-toplevel)
cd "$root"

while :; do
    if [ -e .scratch/tmp/checkpoints-off ]; then
        exit 0
    fi
    git worktree list --porcelain | awk '/^worktree /{print $2}' | while read -r dir; do
        case "$dir" in
            */.scratch/wt/*) ;;
            *) continue ;;
        esac
        if [ -n "$(git -C "$dir" status --porcelain -- . ':(exclude)target' ':(exclude)node_modules' 2>/dev/null)" ]; then
            git -C "$dir" add -A -- . ':(exclude)target' ':(exclude)node_modules' ':(exclude)src/dist' ':(exclude).scratch'
            git -C "$dir" -c user.name="checkpoint" -c user.email="checkpoint@localhost" \
                commit -q -m "[checkpoint] automatic save of work in progress" || true
        fi
    done
    sleep 120
done
