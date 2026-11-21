let inside = p"/srv/app/logs/a.log".relative_to(p"/srv/app")
let outside = p"/etc/hosts".relative_to(p"/srv/app")
print f"inside: {inside.display()}"
print f"outside: {outside.display()}"
