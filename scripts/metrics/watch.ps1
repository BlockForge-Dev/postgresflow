param(
  [string]$BaseUrl = "http://localhost:3003",
  [int]$IntervalSeconds = 1,
  [string]$Queue = ""
)

while ($true) {
  $qs = ""
  if ($Queue) { $qs = "?queue=$Queue" }
  $now = Get-Date -Format "HH:mm:ss"
  try {
    $resp = Invoke-RestMethod "$BaseUrl/metrics$qs"
    if ($resp.queues.Count -gt 0) {
      $row = $resp.queues[0]
      $line = "{0} queue={1} depth={2} jobs_sec={3} success={4} retry={5} mean_ms={6}" -f `
        $now, $row.queue, $row.runnable_queue_depth, $row.jobs_per_sec, $row.success_rate, $row.retry_rate, $row.mean_latency_ms
    } else {
      $line = "{0} no queues" -f $now
    }
    Write-Host $line
  } catch {
    Write-Host "$now error: $($_.Exception.Message)"
  }
  Start-Sleep -Seconds $IntervalSeconds
}
