/** Returns the display icon for a CI check state/conclusion */
export function getCiIcon(state: string): string {
  switch (state) {
    case "SUCCESS":
    case "success":
      return "\u2713";
    case "FAILURE":
    case "failure":
    case "ERROR":
    case "error":
      return "\u2717";
    default:
      return "\u25CF";
  }
}

/** Returns the CSS class for a CI check state/conclusion */
export function getCiClass(state: string): string {
  switch (state) {
    case "SUCCESS":
    case "success":
      return "success";
    case "FAILURE":
    case "failure":
    case "ERROR":
    case "error":
      return "failure";
    default:
      return "pending";
  }
}
