package com.fiambre.neoguitar.tabs;

import android.app.Activity;
import android.content.ActivityNotFoundException;
import android.content.Intent;
import android.graphics.Color;
import android.os.Bundle;
import android.view.Gravity;
import android.view.View;
import android.view.WindowInsets;
import android.view.inputmethod.EditorInfo;
import android.widget.ArrayAdapter;
import android.widget.Button;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.ListView;
import android.widget.TextView;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.File;
import java.util.ArrayList;

/**
 * NeoGuitar Tabs: search a song, tap a result, and the Guitar Pro file is downloaded and opened in NeoGuitar.
 * All the scraping lives in the Rust library (gprotab); this class is just the screen.
 */
public class TabsActivity extends Activity {
    private static final String NEOGUITAR = "com.fiambre.neoguitar";

    private EditText queryBox;
    private Button searchButton, prevButton, nextButton;
    private TextView status, pageLabel;
    private ListView list;
    private final ArrayList<String> ids = new ArrayList<String>();
    private final ArrayList<String> labels = new ArrayList<String>();
    private ArrayAdapter<String> adapter;
    private int page = 1;
    private boolean hasNext = false;
    private boolean busy = false;
    private String lastQuery = "";

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        setTitle("NeoGuitar Tabs");

        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        final int pad = dp(14);
        root.setPadding(pad, pad, pad, pad);
        // Android 15+ draws under the system bars: keep the content clear of them.
        root.setOnApplyWindowInsetsListener(new View.OnApplyWindowInsetsListener() {
            public WindowInsets onApplyWindowInsets(View v, WindowInsets in) {
                v.setPadding(pad + in.getSystemWindowInsetLeft(), pad + in.getSystemWindowInsetTop(),
                        pad + in.getSystemWindowInsetRight(), pad + in.getSystemWindowInsetBottom());
                return in;
            }
        });

        TextView title = new TextView(this);
        title.setText("Find tabs online");
        title.setTextSize(22);
        root.addView(title);

        TextView note = new TextView(this);
        note.setText("Tabs are the work of their authors: use them for your own practice and respect their copyright.");
        note.setTextSize(12);
        note.setTextColor(Color.GRAY);
        note.setPadding(0, dp(2), 0, dp(8));
        root.addView(note);

        LinearLayout row = new LinearLayout(this);
        row.setOrientation(LinearLayout.HORIZONTAL);
        queryBox = new EditText(this);
        queryBox.setHint("Song title…");
        queryBox.setSingleLine(true);
        queryBox.setImeOptions(EditorInfo.IME_ACTION_SEARCH);
        queryBox.setOnEditorActionListener(new TextView.OnEditorActionListener() {
            public boolean onEditorAction(TextView v, int actionId, android.view.KeyEvent e) {
                search(1);
                return true;
            }
        });
        row.addView(queryBox, new LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f));
        searchButton = new Button(this);
        searchButton.setText("Search");
        searchButton.setOnClickListener(new View.OnClickListener() {
            public void onClick(View v) { search(1); }
        });
        row.addView(searchButton);
        root.addView(row);

        status = new TextView(this);
        status.setPadding(0, dp(6), 0, dp(6));
        root.addView(status);

        adapter = new ArrayAdapter<String>(this, android.R.layout.simple_list_item_1, labels);
        list = new ListView(this);
        list.setAdapter(adapter);
        list.setOnItemClickListener(new android.widget.AdapterView.OnItemClickListener() {
            public void onItemClick(android.widget.AdapterView<?> p, View v, int pos, long id) {
                download(ids.get(pos));
            }
        });
        root.addView(list, new LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f));

        LinearLayout pager = new LinearLayout(this);
        pager.setGravity(Gravity.CENTER);
        prevButton = new Button(this);
        prevButton.setText("◀ Previous");
        prevButton.setOnClickListener(new View.OnClickListener() {
            public void onClick(View v) { search(page - 1, lastQuery); }
        });
        pageLabel = new TextView(this);
        pageLabel.setPadding(dp(12), 0, dp(12), 0);
        nextButton = new Button(this);
        nextButton.setText("Next ▶");
        nextButton.setOnClickListener(new View.OnClickListener() {
            public void onClick(View v) { search(page + 1, lastQuery); }
        });
        pager.addView(prevButton);
        pager.addView(pageLabel);
        pager.addView(nextButton);
        root.addView(pager);

        setContentView(root);
        refreshButtons();

        // `am start ... --es q "song title"` opens with that search already done.
        String q = getIntent() == null ? null : getIntent().getStringExtra("q");
        if (q != null && !q.trim().isEmpty()) {
            queryBox.setText(q);
            search(1, q.trim());
        }
    }


    private int dp(int v) {
        return (int) (v * getResources().getDisplayMetrics().density + 0.5f);
    }

    private void refreshButtons() {
        searchButton.setEnabled(!busy);
        prevButton.setEnabled(!busy && page > 1);
        nextButton.setEnabled(!busy && hasNext);
        pageLabel.setText(labels.isEmpty() ? "" : "Page " + page);
        prevButton.setVisibility(labels.isEmpty() ? View.GONE : View.VISIBLE);
        nextButton.setVisibility(labels.isEmpty() ? View.GONE : View.VISIBLE);
    }

    private void setBusy(boolean b, String message) {
        busy = b;
        status.setText(message == null ? "" : message);
        refreshButtons();
    }

    private void search(int p) {
        search(p, queryBox.getText().toString().trim());
    }

    private void search(final int p, final String q) {
        if (busy || q.isEmpty() || p < 1) return;
        setBusy(true, "Searching…");
        new Thread(new Runnable() {
            public void run() {
                String json = Native.run(new String[] {"search", q, String.valueOf(p)});
                try {
                    JSONObject o = new JSONObject(json);
                    if (o.has("error")) throw new Exception(o.getString("error"));
                    final ArrayList<String> newIds = new ArrayList<String>();
                    final ArrayList<String> newLabels = new ArrayList<String>();
                    JSONArray results = o.getJSONArray("results");
                    for (int i = 0; i < results.length(); i++) {
                        JSONObject r = results.getJSONObject(i);
                        newIds.add(r.getString("id"));
                        newLabels.add(r.getString("artist") + " — " + r.getString("title"));
                    }
                    final boolean more = o.optBoolean("has_next", false);
                    runOnUiThread(new Runnable() {
                        public void run() {
                            ids.clear();
                            ids.addAll(newIds);
                            labels.clear();
                            labels.addAll(newLabels);
                            adapter.notifyDataSetChanged();
                            list.setSelection(0);
                            page = p;
                            hasNext = more;
                            lastQuery = q;
                            setBusy(false, newIds.isEmpty() ? "No results." : "Tap a song to download it and open it in NeoGuitar.");
                        }
                    });
                } catch (final Exception e) {
                    runOnUiThread(new Runnable() {
                        public void run() { setBusy(false, "Error: " + e.getMessage()); }
                    });
                }
            }
        }).start();
    }

    private void download(final String id) {
        if (busy) return;
        setBusy(true, "Downloading…");
        final File dir = TabsProvider.tabsDir(this);
        new Thread(new Runnable() {
            public void run() {
                String json = Native.run(new String[] {"download", id, dir.getAbsolutePath()});
                try {
                    JSONObject o = new JSONObject(json);
                    if (o.has("error")) throw new Exception(o.getString("error"));
                    final File file = new File(o.getString("path"));
                    runOnUiThread(new Runnable() {
                        public void run() {
                            setBusy(false, "Downloaded " + file.getName());
                            openInNeoGuitar(file);
                        }
                    });
                } catch (final Exception e) {
                    runOnUiThread(new Runnable() {
                        public void run() { setBusy(false, "Could not download: " + e.getMessage()); }
                    });
                }
            }
        }).start();
    }

    private void openInNeoGuitar(File file) {
        Intent i = new Intent(Intent.ACTION_VIEW);
        i.setDataAndType(TabsProvider.uriFor(file), "application/octet-stream");
        i.setClassName(NEOGUITAR, NEOGUITAR + ".NeoActivity");
        i.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
        try {
            startActivity(i);
        } catch (ActivityNotFoundException e) {
            status.setText("NeoGuitar is not installed.");
        }
    }
}
