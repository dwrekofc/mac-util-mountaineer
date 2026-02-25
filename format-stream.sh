#!/bin/bash
# Formats Claude Code stream-json output into readable terminal output.
# Usage: cat log.jsonl | ./format-stream.sh
#    or: ... | tee log.jsonl | ./format-stream.sh
#
# Shows: thinking, text, tool calls, tool results, system events

# Colors
RESET='\033[0m'
DIM='\033[2m'
BOLD='\033[1m'
CYAN='\033[36m'
YELLOW='\033[33m'
GREEN='\033[32m'
MAGENTA='\033[35m'
BLUE='\033[34m'
RED='\033[31m'
WHITE='\033[97m'

while IFS= read -r line; do
    type=$(echo "$line" | jq -r '.type // empty')

    case "$type" in
        system)
            subtype=$(echo "$line" | jq -r '.subtype // empty')
            if [ "$subtype" = "init" ]; then
                model=$(echo "$line" | jq -r '.model // "unknown"')
                cwd=$(echo "$line" | jq -r '.cwd // "unknown"')
                echo -e "${BOLD}${CYAN}━━━ Session Init ━━━${RESET}"
                echo -e "${DIM}Model: ${model}${RESET}"
                echo -e "${DIM}CWD:   ${cwd}${RESET}"
                echo ""
            fi
            ;;

        assistant)
            # Process each content block in the message
            block_count=$(echo "$line" | jq '.message.content | length')
            for ((i=0; i<block_count; i++)); do
                block_type=$(echo "$line" | jq -r ".message.content[$i].type")

                case "$block_type" in
                    thinking)
                        thinking=$(echo "$line" | jq -r ".message.content[$i].thinking")
                        echo -e "${MAGENTA}${BOLD}[thinking]${RESET}"
                        echo -e "${DIM}${thinking}${RESET}"
                        echo ""
                        ;;

                    text)
                        text=$(echo "$line" | jq -r ".message.content[$i].text")
                        echo -e "${WHITE}${text}${RESET}"
                        echo ""
                        ;;

                    tool_use)
                        tool_name=$(echo "$line" | jq -r ".message.content[$i].name")
                        # Show tool-specific summary
                        case "$tool_name" in
                            Read)
                                file=$(echo "$line" | jq -r ".message.content[$i].input.file_path // empty")
                                echo -e "${YELLOW}[tool] ${BOLD}Read${RESET} ${DIM}${file}${RESET}"
                                ;;
                            Write)
                                file=$(echo "$line" | jq -r ".message.content[$i].input.file_path // empty")
                                echo -e "${YELLOW}[tool] ${BOLD}Write${RESET} ${DIM}${file}${RESET}"
                                ;;
                            Edit)
                                file=$(echo "$line" | jq -r ".message.content[$i].input.file_path // empty")
                                echo -e "${YELLOW}[tool] ${BOLD}Edit${RESET} ${DIM}${file}${RESET}"
                                ;;
                            Bash)
                                cmd=$(echo "$line" | jq -r ".message.content[$i].input.command // empty" | head -1)
                                desc=$(echo "$line" | jq -r ".message.content[$i].input.description // empty")
                                if [ -n "$desc" ]; then
                                    echo -e "${YELLOW}[tool] ${BOLD}Bash${RESET} ${DIM}${desc}${RESET}"
                                else
                                    echo -e "${YELLOW}[tool] ${BOLD}Bash${RESET} ${DIM}${cmd}${RESET}"
                                fi
                                ;;
                            Glob)
                                pattern=$(echo "$line" | jq -r ".message.content[$i].input.pattern // empty")
                                echo -e "${YELLOW}[tool] ${BOLD}Glob${RESET} ${DIM}${pattern}${RESET}"
                                ;;
                            Grep)
                                pattern=$(echo "$line" | jq -r ".message.content[$i].input.pattern // empty")
                                echo -e "${YELLOW}[tool] ${BOLD}Grep${RESET} ${DIM}${pattern}${RESET}"
                                ;;
                            Task)
                                desc=$(echo "$line" | jq -r ".message.content[$i].input.description // empty")
                                agent=$(echo "$line" | jq -r ".message.content[$i].input.subagent_type // empty")
                                echo -e "${BLUE}[agent] ${BOLD}${agent}${RESET} ${DIM}${desc}${RESET}"
                                ;;
                            TodoWrite)
                                echo -e "${YELLOW}[tool] ${BOLD}TodoWrite${RESET}"
                                ;;
                            TaskCreate|TaskUpdate|TaskList|TaskGet)
                                echo -e "${YELLOW}[tool] ${BOLD}${tool_name}${RESET}"
                                ;;
                            WebSearch)
                                query=$(echo "$line" | jq -r ".message.content[$i].input.query // empty")
                                echo -e "${YELLOW}[tool] ${BOLD}WebSearch${RESET} ${DIM}${query}${RESET}"
                                ;;
                            WebFetch)
                                url=$(echo "$line" | jq -r ".message.content[$i].input.url // empty")
                                echo -e "${YELLOW}[tool] ${BOLD}WebFetch${RESET} ${DIM}${url}${RESET}"
                                ;;
                            *)
                                echo -e "${YELLOW}[tool] ${BOLD}${tool_name}${RESET}"
                                ;;
                        esac
                        ;;
                esac
            done
            ;;

        user)
            # Check if it's tool results or user text
            block_count=$(echo "$line" | jq '.message.content | length')
            for ((i=0; i<block_count; i++)); do
                block_type=$(echo "$line" | jq -r ".message.content[$i].type")

                case "$block_type" in
                    tool_result)
                        is_error=$(echo "$line" | jq -r ".message.content[$i].is_error // false")
                        content=$(echo "$line" | jq -r ".message.content[$i].content // empty" | head -3)
                        if [ "$is_error" = "true" ]; then
                            echo -e "${RED}  [result] ERROR: ${content}${RESET}"
                        else
                            # Truncate long results
                            preview=$(echo "$content" | head -c 200)
                            if [ ${#content} -gt 200 ]; then
                                preview="${preview}..."
                            fi
                            echo -e "${GREEN}  [result]${RESET} ${DIM}${preview}${RESET}"
                        fi
                        ;;
                    text)
                        text=$(echo "$line" | jq -r ".message.content[$i].text")
                        echo -e "${CYAN}[user] ${text}${RESET}"
                        ;;
                esac
            done
            ;;

        rate_limit_event)
            status=$(echo "$line" | jq -r '.rate_limit_info.status // empty')
            echo -e "${DIM}[rate_limit] ${status}${RESET}"
            ;;
    esac
done
