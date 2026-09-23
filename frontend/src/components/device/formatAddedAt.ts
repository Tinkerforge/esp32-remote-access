export function formatAddedAt(timestamp?: number | null): string {
    return timestamp == null ? "-" : new Date(timestamp * 1000).toLocaleString();
}
