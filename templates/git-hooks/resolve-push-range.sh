#!/usr/bin/env bash
# resolve-push-range.sh — reusable pre-push range resolver.
#
# Extracted from minibox's .githooks/pre-push. Prints the commit range(s)
# being pushed, one "base..tip" per ref, for downstream tools (secret
# scanners, diff linters, etc.) to inspect. Has no project-specific
# dependencies — copy into any repo's .githooks/pre-push (or source it from
# there) and append your own gate command after resolve_range.
#
# Git calls a pre-push hook with:
#   $1  remote name
#   $2  remote URL
# Stdin contains one line per ref being pushed:
#   <local_ref> <local_oid> <remote_ref> <remote_oid>
#
# Range resolution rules:
#   local_oid  == zero  → deletion, skip (nothing to inspect)
#   remote_oid == zero  → new branch: base = merge-base against main (fallback: initial commit)
#   otherwise           → existing branch: base = remote_oid

set -eou pipefail

# fail <title> <detail>
# Print a formatted error to stderr and exit 1.
fail() {
    if test -t 2; then
        printf '\x1b[1;31;49m%s\x1b[0m\n%s\n' "$1" "$2" >&2
    else
        printf '%s\n%s\n' "$1" "$2" >&2
    fi
    exit 1
}

# zero_oid
# Returns the all-zeros sentinel OID git uses for non-existent refs.
zero_oid() {
    git hash-object --stdin </dev/null | tr '0-9a-f' '0'
}

# merge_base <oid>
# Returns the merge-base of <oid> against main.
# Falls back to the initial commit if main doesn't exist.
merge_base() {
    git merge-base "$1" main 2>/dev/null \
        || git rev-list --max-parents=0 HEAD
}

# resolve_range
# Reads git's stdin ref lines and prints one resolved range per ref.
resolve_range() {
    local zero
    zero=$(zero_oid)

    local _local_ref local_oid _remote_ref remote_oid
    while read -r _local_ref local_oid _remote_ref remote_oid; do
        if [ "${local_oid}" = "${zero}" ]; then
            # Branch deletion — nothing to inspect.
            continue
        fi

        local base
        if [ "${remote_oid}" = "${zero}" ]; then
            # New branch — inspect commits since divergence from main.
            base=$(merge_base "${local_oid}")
        else
            # Existing branch update — inspect only the new commits.
            base="${remote_oid}"
        fi

        echo "${base}..${local_oid}"
    done
}

main() {
    resolve_range

    # Quality gate — replace with this project's own gate command
    # (e.g. `cargo xtask prepush`, `taskit check`, `just verify`).
    # cargo xtask prepush || exit 1
}

main "$@"
