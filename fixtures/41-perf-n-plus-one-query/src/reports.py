import sqlite3


def get_order_details(conn: sqlite3.Connection) -> list[dict]:
    """Fetch all orders with customer names for the dashboard."""
    orders = conn.execute("SELECT id, customer_id, total FROM orders").fetchall()

    results = []
    for order in orders:
        customer = conn.execute(
            "SELECT name FROM customers WHERE id = ?", (order[1],)
        ).fetchone()
        results.append({
            "order_id": order[0],
            "customer": customer[0] if customer else "Unknown",
            "total": order[2],
        })
    return results
