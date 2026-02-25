#!/bin/bash
# Usage: ./loop.sh [mode] [max_iterations]
# Examples:
#   ./loop.sh              # Build mode, unlimited iterations
#   ./loop.sh 20           # Build mode, max 20 iterations
#   ./loop.sh plan         # Plan mode, unlimited iterations
#   ./loop.sh plan 5       # Plan mode, max 5 iterations
#   ./loop.sh audit 10     # Custom mode, max 10 iterations
#   ./loop.sh help         # List available modes
#
# Any PROMPT_<name>.md file is auto-discovered as a mode.
# Readable streaming output + raw JSON logs.
# Requires: jq

# Parse arguments
if [ -z "$1" ]; then
    MODE="build"
    PROMPT_FILE="PROMPT_build.md"
    MAX_ITERATIONS=0
elif [[ "$1" =~ ^[0-9]+$ ]]; then
    MODE="build"
    PROMPT_FILE="PROMPT_build.md"
    MAX_ITERATIONS=$1
elif [ "$1" = "help" ]; then
    echo "Usage: ./loop.sh [mode] [max_iterations]"
    echo ""
    echo "Available modes:"
    for f in PROMPT_*.md; do
        [ -f "$f" ] || continue
        name="${f#PROMPT_}"
        name="${name%.md}"
        desc=$(head -1 "$f" | sed -n 's/^<!-- description: \(.*\) -->/\1/p')
        if [ -n "$desc" ]; then
            printf "  %-12s %s\n" "$name" "$desc"
        else
            printf "  %-12s\n" "$name"
        fi
    done
    echo ""
    echo "Default mode is 'build' when no mode is specified."
    exit 0
else
    MODE="$1"
    PROMPT_FILE="PROMPT_${MODE}.md"
    MAX_ITERATIONS=${2:-0}
fi

ITERATION=0
CURRENT_BRANCH=$(git branch --show-current)
LOG_DIR="logs"
mkdir -p "$LOG_DIR"

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Mode:   $MODE"
echo "Prompt: $PROMPT_FILE"
echo "Branch: $CURRENT_BRANCH"
echo "Logs:   $LOG_DIR/"
[ $MAX_ITERATIONS -gt 0 ] && echo "Max:    $MAX_ITERATIONS iterations"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Verify prompt file exists
if [ ! -f "$PROMPT_FILE" ]; then
    echo "Error: $PROMPT_FILE not found"
    exit 1
fi

# Verify jq is available
if ! command -v jq &> /dev/null; then
    echo "Error: jq is required but not installed. Install with: brew install jq"
    exit 1
fi

while true; do
    if [ $MAX_ITERATIONS -gt 0 ] && [ $ITERATION -ge $MAX_ITERATIONS ]; then
        echo "Reached max iterations: $MAX_ITERATIONS"
        break
    fi

    TIMESTAMP=$(date +%Y%m%d_%H%M%S)
    LOG_FILE="${LOG_DIR}/iteration_${ITERATION}_${TIMESTAMP}.jsonl"

    echo -e "\n--- Iteration $ITERATION started at $(date) ---"
    echo "Log: $LOG_FILE"

    # Run Claude with stream-json, tee raw JSON to log, format for readable output.
    # format-stream.sh shows thinking, text, tool calls, and results with colors.
    cat "$PROMPT_FILE" | claude -p \
        --dangerously-skip-permissions \
        --output-format=stream-json \
        --model opus \
        --verbose \
        | tee "$LOG_FILE" \
        | ./format-stream.sh

    echo -e "\n--- Iteration $ITERATION finished at $(date) ---"

    # Push changes after each iteration
    git push origin "$CURRENT_BRANCH" || {
        echo "Failed to push. Creating remote branch..."
        git push -u origin "$CURRENT_BRANCH"
    }

    ITERATION=$((ITERATION + 1))
    echo -e "\n\n======================== LOOP $ITERATION ========================\n"
done
