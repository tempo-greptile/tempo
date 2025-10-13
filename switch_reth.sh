#!/bin/bash

# Script to easily switch between different reth versions for testing

set -e

CURRENT_COMMIT="1619408"
OLD_COMMIT="c02a68dc78eb4e080288ed6779439fd1d3169667"

print_usage() {
    echo "Usage: ./switch_reth.sh [main|current|old]"
    echo ""
    echo "Options:"
    echo "  main     - Switch to reth main branch"
    echo "  current  - Switch to current commit $CURRENT_COMMIT"
    echo "  old      - Switch to old commit $OLD_COMMIT"
    echo ""
    echo "After switching, run: cargo update -p reth && cargo build"
}

if [ $# -eq 0 ]; then
    print_usage
    exit 1
fi

case "$1" in
    main)
        echo "Switching to reth main branch..."
        # Replace rev with branch = "main"
        sed -i '' 's/git = "https:\/\/github.com\/paradigmxyz\/reth", rev = "[^"]*"/git = "https:\/\/github.com\/paradigmxyz\/reth", branch = "main"/g' Cargo.toml
        echo "✓ Switched to main branch"
        ;;
    current)
        echo "Switching to current commit $CURRENT_COMMIT..."
        # Replace with the current commit
        sed -i '' 's/git = "https:\/\/github.com\/paradigmxyz\/reth", branch = "main"/git = "https:\/\/github.com\/paradigmxyz\/reth", rev = "'$CURRENT_COMMIT'"/g' Cargo.toml
        sed -i '' 's/git = "https:\/\/github.com\/paradigmxyz\/reth", rev = "[^"]*"/git = "https:\/\/github.com\/paradigmxyz\/reth", rev = "'$CURRENT_COMMIT'"/g' Cargo.toml
        echo "✓ Switched to current commit"
        ;;
    old)
        echo "Switching to old commit $OLD_COMMIT..."
        # Replace with the old commit
        sed -i '' 's/git = "https:\/\/github.com\/paradigmxyz\/reth", branch = "main"/git = "https:\/\/github.com\/paradigmxyz\/reth", rev = "'$OLD_COMMIT'"/g' Cargo.toml
        sed -i '' 's/git = "https:\/\/github.com\/paradigmxyz\/reth", rev = "[^"]*"/git = "https:\/\/github.com\/paradigmxyz\/reth", rev = "'$OLD_COMMIT'"/g' Cargo.toml
        echo "✓ Switched to old commit"
        ;;
    *)
        echo "Error: Unknown option '$1'"
        print_usage
        exit 1
        ;;
esac

echo ""
echo "Next steps:"
echo "  1. cargo update -p reth"
echo "  2. cargo build"
echo ""
echo "Or run both together:"
echo "  cargo update -p reth && cargo build"
