#[derive(Debug, Clone)]
pub struct SuppressScope {
    pub start: usize,
    pub end: usize,
}

pub fn find_suppress_if_scopes(body: &[u8]) -> Vec<SuppressScope> {
    let mut scopes = vec![];
    let mut i = 0;
    while i + 2 <= body.len() {
        if body[i] == 0x0A && body[i + 1] & 0x80 != 0 {
            let scope_start = i + 2;
            let mut depth = 1;
            let mut j = scope_start;
            while j + 2 <= body.len() {
                if body[j] == 0x0A && body[j + 1] & 0x80 != 0 {
                    depth += 1;
                } else if body[j] == 0x29 && body[j + 1] == 0x02 {
                    depth -= 1;
                    if depth == 0 {
                        scopes.push(SuppressScope {
                            start: scope_start,
                            end: j,
                        });
                        break;
                    }
                }
                let len = if body.len() > j + 1 {
                    body[j + 1] as usize & 0x7F
                } else {
                    2
                };
                j += len.max(2);
            }
            i = j + 2;
        } else {
            i += 1;
        }
    }
    scopes
}

pub fn unsuppress(ifr: &mut Vec<u8>, scope: &SuppressScope) {
    if scope.end + 2 <= ifr.len() && ifr[scope.end] == 0x29 && ifr[scope.end + 1] == 0x02 {
        ifr.drain(scope.end..scope.end + 2);
    }
    ifr.splice(scope.start..scope.start, [0x29, 0x02]);
}
