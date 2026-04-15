public class Product {
    private String name;
    private int priceCents;
    private int quantity;

    public Product(String name, int priceCents, int quantity) {
        this.name = name != null ? name : "";
        if (priceCents < 0) throw new IllegalArgumentException("Price cannot be negative");
        if (quantity < 0) throw new IllegalArgumentException("Quantity cannot be negative");
        this.priceCents = priceCents;
        this.quantity = quantity;
    }

    public String getName() {
        return this.name;
    }

    public void setName(String name) {
        this.name = name != null ? name : "";
    }

    public int getPriceCents() {
        return this.priceCents;
    }

    public void setPriceCents(int priceCents) {
        if (priceCents < 0) throw new IllegalArgumentException("Price cannot be negative");
        this.priceCents = priceCents;
    }

    public int getQuantity() {
        return this.quantity;
    }

    public void setQuantity(int quantity) {
        if (quantity < 0) throw new IllegalArgumentException("Quantity cannot be negative");
        this.quantity = quantity;
    }

    @Override
    public String toString() {
        return "Product{name='" + name + "', priceCents=" + priceCents + ", qty=" + quantity + "}";
    }
}
