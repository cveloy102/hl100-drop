#ifndef HL100_H
#define HL100_H
#ifdef __cplusplus
extern "C" {
#endif
/* Returns an allocated UTF-8 JSON string. Always release it using hl100_free.
   Commands and response schema: docs/protocol.md. Calls are serialized internally. */
char *hl100_command(const char *command);
void hl100_free(char *value);
#ifdef __cplusplus
}
#endif
#endif
