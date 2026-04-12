import java.util.List;
import java.util.ArrayList;

public class Repository {
    private final List<String> records = new ArrayList<>();

    public List<String> fetchRecords() {
        return List.copyOf(records);
    }

    public void addRecord(String record) {
        records.add(record);
    }

    public int getRecordCount() {
        return records.size();
    }
}
