interface User {
  id: number;
  name: string;
  active: boolean;
}

async function fetchUser(id: number): Promise<User> {
  const response = await fetch(`/api/users/${id}`);
  return response.json();
}

export async function deactivateUser(id: number): Promise<string> {
  const user = fetchUser(id); // missing await
  if (user.active) {
    return `Deactivated ${user.name}`;
  }
  return `User ${user.name} already inactive`;
}
