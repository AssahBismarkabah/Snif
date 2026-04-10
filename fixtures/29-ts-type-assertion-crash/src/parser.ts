interface ApiResponse {
  data: { items: string[] };
  status: number;
}

export function extractItems(raw: unknown): string[] {
  const response = raw as ApiResponse;
  return response.data.items;
}

export function extractItemsSafe(raw: unknown): string[] {
  if (
    typeof raw === "object" && raw !== null &&
    "data" in raw && (raw as any).data !== null && typeof (raw as any).data === "object" &&
    Array.isArray((raw as any).data.items)
  ) {
    return (raw as ApiResponse).data.items;
  }
  return [];
}
