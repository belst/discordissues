create table mapping (
    thread_id INT8 not null,
    issue_nr INT8 not null,
    primary key (thread_id, issue_nr)
);
