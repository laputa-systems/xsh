def service_oracle(before, after, notified):
    return (before.get('pid', 0) == 0 and before.get('desired') == 'down' and
            after.get('pid', 0) > 0 and after.get('ready') is True and
            after.get('state') == 'running' and after.get('pid') == notified)
