package com.fiambre.neoguitar;

import android.app.AlertDialog;
import android.app.NativeActivity;
import android.content.DialogInterface;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.os.Bundle;
import android.provider.OpenableColumns;
import android.text.InputType;
import android.view.WindowManager;
import android.widget.EditText;

import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;

/**
 * The only Java in the app. NativeActivity cannot receive the result of the system file picker, so this subclass
 * opens the picker, copies the chosen document to private storage (the Rust side reads plain files) and leaves
 * the path for Rust to collect with takePending(). It also accepts files sent to the app ("Open with" / "Share").
 *
 * Result format: "<kind>|<absolute path>", "C|" when the picker was cancelled, "E|<message>" on failure or
 * "t<id>|<text>" for promptText().
 * Kinds: 0 = song, 1 = SoundFont, 2 = lyrics.
 */
public class NeoActivity extends NativeActivity {
    private static final int REQUEST_PICK = 7001;
    private int pickKind = 0;
    private String pending;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        handleIntent(getIntent());
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        handleIntent(intent);
    }

    private void handleIntent(Intent intent) {
        if (intent == null) return;
        Uri uri = null;
        if (Intent.ACTION_VIEW.equals(intent.getAction())) {
            uri = intent.getData();
        } else if (Intent.ACTION_SEND.equals(intent.getAction())) {
            uri = (Uri) intent.getParcelableExtra(Intent.EXTRA_STREAM);
        }
        if (uri != null) {
            copyAsync(uri, -1);
            // The same intent must not be handled again if the activity is recreated.
            intent.setAction(Intent.ACTION_MAIN);
        }
    }

    /** Called from Rust (any thread). */
    public void pickFile(final int kind) {
        runOnUiThread(new Runnable() {
            public void run() {
                pickKind = kind;
                Intent i = new Intent(Intent.ACTION_OPEN_DOCUMENT);
                i.addCategory(Intent.CATEGORY_OPENABLE);
                i.setType("*/*");
                try {
                    startActivityForResult(i, REQUEST_PICK);
                } catch (Exception e) {
                    setPending("E|" + e);
                }
            }
        });
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode != REQUEST_PICK) return;
        if (resultCode != RESULT_OK || data == null || data.getData() == null) {
            setPending("C|");
            return;
        }
        copyAsync(data.getData(), pickKind);
    }

    /**
     * Native text box. egui's own text fields fight with the Android keyboard (the keyboard flickers), so the
     * app asks the system for the text instead. The answer arrives as "t<id>|<text>"; cancelling gives "C|".
     */
    public void promptText(final int id, final String title, final String initial) {
        runOnUiThread(new Runnable() {
            public void run() {
                final EditText input = new EditText(NeoActivity.this);
                input.setInputType(InputType.TYPE_CLASS_TEXT);
                input.setSingleLine(true);
                input.setText(initial);
                input.setSelection(input.getText().length());
                AlertDialog.Builder b = new AlertDialog.Builder(NeoActivity.this);
                b.setTitle(title);
                b.setView(input);
                b.setPositiveButton("OK", new DialogInterface.OnClickListener() {
                    public void onClick(DialogInterface d, int which) {
                        setPending("t" + id + "|" + input.getText().toString());
                    }
                });
                b.setNegativeButton("Cancel", new DialogInterface.OnClickListener() {
                    public void onClick(DialogInterface d, int which) {
                        setPending("C|");
                    }
                });
                b.setOnCancelListener(new DialogInterface.OnCancelListener() {
                    public void onCancel(DialogInterface d) {
                        setPending("C|");
                    }
                });
                AlertDialog dialog = b.create();
                dialog.getWindow().setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_STATE_VISIBLE);
                dialog.show();
                input.requestFocus();
            }
        });
    }

    /** Called from Rust: the last result, or null. */
    public synchronized String takePending() {
        String p = pending;
        pending = null;
        return p;
    }

    private synchronized void setPending(String p) {
        pending = p;
    }

    private String displayName(Uri uri) {
        String name = null;
        try (Cursor c = getContentResolver().query(uri, null, null, null, null)) {
            if (c != null && c.moveToFirst()) {
                int idx = c.getColumnIndex(OpenableColumns.DISPLAY_NAME);
                if (idx >= 0) name = c.getString(idx);
            }
        } catch (Exception ignored) {
        }
        if (name == null || name.isEmpty()) name = uri.getLastPathSegment();
        if (name == null || name.isEmpty()) name = "file";
        return name.replaceAll("[\\\\/:*?\"<>|]", "_");
    }

    private static int kindOf(String name, int requested) {
        if (requested >= 0) return requested;
        String n = name.toLowerCase();
        if (n.endsWith(".sf2")) return 1;
        if (n.endsWith(".lrc") || n.endsWith(".txt")) return 2;
        return 0;
    }

    private void copyAsync(final Uri uri, final int requestedKind) {
        new Thread(new Runnable() {
            public void run() {
                try {
                    String name = displayName(uri);
                    int kind = kindOf(name, requestedKind);
                    File base = kind == 2 ? getCacheDir() : getFilesDir();
                    File dir = new File(base, kind == 1 ? "soundfonts" : kind == 2 ? "lyrics-files" : "songs");
                    dir.mkdirs();
                    File out = new File(dir, name);
                    try (InputStream in = getContentResolver().openInputStream(uri);
                         OutputStream os = new FileOutputStream(out)) {
                        if (in == null) throw new Exception("could not open the file");
                        byte[] buf = new byte[1 << 16];
                        int n;
                        while ((n = in.read(buf)) > 0) os.write(buf, 0, n);
                    }
                    setPending(kind + "|" + out.getAbsolutePath());
                } catch (Exception e) {
                    setPending("E|" + e.getMessage());
                }
            }
        }).start();
    }
}