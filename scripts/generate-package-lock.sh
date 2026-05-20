#!/bin/bash
# Generate package-lock.json
# Requires Node.js (npm) installed

set -e

echo "🔨 Generating package-lock.json..."

cd "$(dirname "$0")/frontend"

if [ ! -f package-lock.json ]; then
    echo "   package-lock.json not found, generating..."
    npm install --package-lock-only
    echo "✅ package-lock.json generated"
else
    echo "   package-lock.json exists, skipping"
fi

if [ -f package-lock.json ]; then
    echo "✅ package-lock.json verified"
    echo "   File size: $(wc -c < package-lock.json) bytes"
else
    echo "❌ package-lock.json generation failed"
    exit 1
fi

echo ""
echo "💡 Tip: Commit the generated package-lock.json:"
echo "   git add frontend/package-lock.json"
echo "   git commit -m 'chore: add package-lock.json for reproducible builds'"