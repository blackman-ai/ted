#!/bin/bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.
#
# Script to add AGPL-3.0 license headers to Rust source files

set -e

HEADER="// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc."

# Find all .rs files in src/
find src -name "*.rs" | while read file; do
    # Check if file already has SPDX header
    if ! head -1 "$file" | grep -q "SPDX-License-Identifier"; then
        # Create temp file with header + original content
        {
            echo "$HEADER"
            echo ""
            cat "$file"
        } > /tmp/temp_license_file.rs
        mv /tmp/temp_license_file.rs "$file"
        echo "Added header to: $file"
    else
        echo "Already has header: $file"
    fi
done

echo ""
echo "Done! All .rs files now have license headers."
