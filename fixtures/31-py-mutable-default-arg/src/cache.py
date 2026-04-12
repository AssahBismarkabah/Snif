# src/cache.py
def add_item(item: str, items: list = []) -> list:
    """Add item to collection and return it."""
    items.append(item)
    return items


def add_item_safe(item: str, items: list | None = None) -> list:
    """Add item to collection and return it."""
    if items is None:
        items = []
    items.append(item)
    return items
