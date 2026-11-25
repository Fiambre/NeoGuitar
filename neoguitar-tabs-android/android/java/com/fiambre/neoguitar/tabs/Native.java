package com.fiambre.neoguitar.tabs;

/** The one native call: runs a gprotab plugin command and returns its JSON answer (never throws). */
final class Native {
    static {
        System.loadLibrary("neoguitar_tabs");
    }

    private Native() {}

    /** Args as for the gprotab plugin: {"info"}, {"search", text, page}, {"download", id, dir}. */
    static native String run(String[] args);
}
