/* An inert PTY peer for synthetic transcript measurements. It never invokes a model.
 * The local binary supplies a readable initial CLAUDE_CONFIG_DIR to TUIC's
 * process-environment API. The preflight rejects an unresolved binding.
 */
#include <stdio.h>
int main(void) {
    while (getchar() != EOF) {}
    return 0;
}
