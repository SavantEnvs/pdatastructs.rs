/* Disable LeakSanitizer at build time (SPEC §6.2 item 15); ASan stays fully active. */
int __lsan_is_turned_off(void) { return 1; }
