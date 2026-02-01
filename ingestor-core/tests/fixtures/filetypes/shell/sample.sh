#!/bin/bash
#
# Sample shell script for testing.
#

set -euo pipefail

# Configuration
readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly LOG_FILE="/var/log/sample.log"
readonly VERSION="1.0.0"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m' # No Color

# Logging functions
log_info() {
    echo -e "${GREEN}[INFO]${NC} $*"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $*" >&2
}

# Show usage
usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS] COMMAND

Commands:
    start       Start the service
    stop        Stop the service
    status      Show service status

Options:
    -h, --help      Show this help
    -v, --version   Show version
    -d, --debug     Enable debug mode
EOF
}

# Start service
start_service() {
    log_info "Starting service..."
    # Simulated start
    sleep 1
    log_info "Service started"
}

# Stop service
stop_service() {
    log_info "Stopping service..."
    sleep 1
    log_info "Service stopped"
}

# Check status
check_status() {
    log_info "Checking status..."
    echo "Service is running"
}

# Parse arguments
main() {
    local debug=false

    while [[ $# -gt 0 ]]; do
        case "$1" in
            -h|--help)
                usage
                exit 0
                ;;
            -v|--version)
                echo "Version: $VERSION"
                exit 0
                ;;
            -d|--debug)
                debug=true
                shift
                ;;
            start)
                start_service
                exit 0
                ;;
            stop)
                stop_service
                exit 0
                ;;
            status)
                check_status
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                usage
                exit 1
                ;;
        esac
    done

    usage
    exit 1
}

main "$@"
