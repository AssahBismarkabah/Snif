interface ApiResponse {
  data: { items: string[] };
  status: number;
}

export function extractItems(raw: unknown): string[] {
  const response = raw as ApiResponse;
  return response.data.items;
}
