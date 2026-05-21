#!/bin/bash
set -euo pipefail

# ANSI color codes (only if outputting to a terminal)
if [ -t 1 ]; then
    RED='\e[1;31m'
    GREEN='\e[1;32m'
    YELLOW='\e[1;33m'
    BLUE='\e[1;34m'
    CYAN='\e[1;36m'
    BOLD='\e[1m'
    NC='\e[0m' # No Color
else
    RED=''
    GREEN=''
    YELLOW=''
    BLUE=''
    CYAN=''
    BOLD=''
    NC=''
fi

readonly BUNDLE_URL="https://github.com/theguy000/stremio-lightning/releases/latest/download/Stremio_Lightning_Linux-x86_64.flatpak"

# Setup temporary file and logs
tmp="$(mktemp --suffix=.flatpak)"
log_file="$(mktemp)"
trap 'rm -f "$tmp" "$log_file"' EXIT

# Helper for showing a loading spinner (only if interactive TTY)
show_spinner() {
    local pid=$1
    local message=$2
    local delay=0.08
    local frames=("⠋" "⠙" "⠹" "⠸" "⠼" "⠴" "⠦" "⠧" "⠇" "⠏")
    
    if [ -t 1 ]; then
        tput civis 2>/dev/null || true
        while kill -0 "$pid" 2>/dev/null; do
            for frame in "${frames[@]}"; do
                if ! kill -0 "$pid" 2>/dev/null; then break; fi
                printf "\r${CYAN}%s${NC} %s" "$frame" "$message"
                sleep $delay
            done
        done
        wait "$pid"
        local exit_code=$?
        tput cnorm 2>/dev/null || true
        
        if [ $exit_code -eq 0 ]; then
            printf "\r${GREEN}[OK]${NC} %s... ${GREEN}Done!${NC}\n" "$message"
            return 0
        else
            printf "\r${RED}[ERR]${NC} %s... ${RED}Failed!${NC}\n" "$message"
            return $exit_code
        fi
    else
        echo "$message..."
        wait "$pid"
        local exit_code=$?
        if [ $exit_code -eq 0 ]; then
            echo "...Done!"
            return 0
        else
            echo "...Failed!"
            return $exit_code
        fi
    fi
}

# Helper for showing a beautiful progress bar during download
show_progress() {
    local pid=$1
    local filepath=$2
    local total_bytes=$3
    local message=$4
    local delay=0.1
    
    if [ -t 1 ]; then
        tput civis 2>/dev/null || true
        
        local frame_idx=0
        local frames=("⠋" "⠙" "⠹" "⠸" "⠼" "⠴" "⠦" "⠧" "⠇" "⠏")
        local num_frames=${#frames[@]}
        
        while kill -0 "$pid" 2>/dev/null; do
            local current_bytes=0
            if [ -f "$filepath" ]; then
                current_bytes=$(stat -c%s "$filepath" 2>/dev/null || stat -f%z "$filepath" 2>/dev/null || echo 0)
            fi
            
            local percent=0
            if [ "$total_bytes" -gt 0 ]; then
                percent=$((current_bytes * 100 / total_bytes))
            fi
            if [ "$percent" -gt 100 ]; then percent=100; fi
            
            # Format sizes to 1 decimal place using pure Bash
            local current_mb_int=$((current_bytes / 1048576))
            local current_mb_dec=$(( (current_bytes % 1048576) * 10 / 1048576 ))
            local current_mb="${current_mb_int}.${current_mb_dec}"
            
            local total_mb_int=$((total_bytes / 1048576))
            local total_mb_dec=$(( (total_bytes % 1048576) * 10 / 1048576 ))
            local total_mb="${total_mb_int}.${total_mb_dec}"
            
            # Build bar
            local filled=$((percent / 5))
            local unfilled=$((20 - filled))
            
            local bar=""
            for ((i=0; i<filled; i++)); do bar="${bar}█"; done
            for ((i=0; i<unfilled; i++)); do bar="${bar}░"; done
            
            # Get current spinner frame and advance index
            local frame="${frames[frame_idx]}"
            frame_idx=$(( (frame_idx + 1) % num_frames ))
            
            printf "\r${CYAN}%s${NC} %s... [${CYAN}%s${NC}] %3d%% (%s/%s MB)" "$frame" "$message" "$bar" "$percent" "$current_mb" "$total_mb"
            sleep $delay
        done
        
        wait "$pid"
        local exit_code=$?
        tput cnorm 2>/dev/null || true
        
        if [ $exit_code -eq 0 ]; then
            local bar="████████████████████"
            local total_mb_int=$((total_bytes / 1048576))
            local total_mb_dec=$(( (total_bytes % 1048576) * 10 / 1048576 ))
            local total_mb="${total_mb_int}.${total_mb_dec}"
            printf "\r${GREEN}[OK]${NC} %s... [${GREEN}%s${NC}] 100%% (%s/%s MB) ${GREEN}Done!${NC}\n" "$message" "$bar" "$total_mb" "$total_mb"
            return 0
        else
            printf "\n${RED}[ERR]${NC} %s... ${RED}Failed!${NC}\n" "$message"
            return $exit_code
        fi
    else
        echo "$message..."
        wait "$pid"
        local exit_code=$?
        if [ $exit_code -eq 0 ]; then
            echo "...Done!"
            return 0
        else
            echo "...Failed!"
            return $exit_code
        fi
    fi
}

# Print beautiful header
echo -e "${CYAN}${BOLD}"
echo "  STREMIO LIGHTNING INSTALLER"
echo "  ==========================="
echo -e "${NC}"

# Step 1: Remote repository setup
echo -e "${BLUE}[1/3]${NC} Preparing Flatpak environment..."
flatpak remote-add --user --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo > /dev/null 2> "$log_file" &
if ! show_spinner $! "Configuring Flathub repository"; then
    echo -e "${RED}Error configuring Flathub repository:${NC}"
    cat "$log_file"
    exit 1
fi

# Step 2: Download Flatpak bundle with progress bar
echo -e "${BLUE}[2/3]${NC} Retrieving Stremio Lightning..."

# Fetch the redirected content length in the background first, or fall back to an estimate
total_bytes=$(curl -sIL "$BUNDLE_URL" | grep -i "^content-length:" | tail -n 1 | awk '{print $2}' | tr -d '\r' || echo 0)
total_bytes=$(echo "$total_bytes" | tr -cd '0-9')
if [ -z "$total_bytes" ] || [ "$total_bytes" -eq 0 ]; then
    total_bytes=192257000 # Fallback close to 183 MB
fi

curl -fsSL -L -o "$tmp" "$BUNDLE_URL" > /dev/null 2> "$log_file" &
if ! show_progress $! "$tmp" "$total_bytes" "Downloading latest release bundle"; then
    echo -e "${RED}Error downloading Flatpak bundle:${NC}"
    cat "$log_file"
    exit 1
fi

# Step 3: Install
echo -e "${BLUE}[3/3]${NC} Installing application..."
flatpak install --user --bundle "$tmp" -y > /dev/null 2> "$log_file" &
if ! show_spinner $! "Installing Stremio Lightning Flatpak"; then
    echo -e "${RED}Installation failed:${NC}"
    cat "$log_file"
    exit 1
fi

echo -e "\n${GREEN}${BOLD}Stremio Lightning has been successfully installed!${NC}"
echo -e "You can launch it from your application menu or by running:"
echo -e "  ${CYAN}flatpak run io.github.theguy000.StremioLightning${NC}\n"
