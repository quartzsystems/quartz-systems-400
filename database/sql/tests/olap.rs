// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Queries at the 7.5 level, against what pub400 gave for the same
//! statements (tools/conformance/sql/cases/query/): common table
//! expressions, recursive ones too, joins with USING, LATERAL and full
//! outer joins, row limits, OLAP specifications, grouping sets, the
//! statistical and ordered aggregates, date arithmetic, row-value
//! predicates and result column types.

#![allow(clippy::too_many_lines)]

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use sql::{Naming, Outcome, SqlContext, SqlError, prepare};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, types};
use store::{FormatParams, Store, StoreConfig};

struct Db {
    store: Store,
    ctx: SqlContext,
}

impl Db {
    fn new() -> Self {
        let d = Arc::new(MemBlockDev::with_seed(16384, 11));
        let params = FormatParams {
            asp_number: 1,
            asp_uuid: [4; 16],
            unit_uuids: vec![[5; 16]],
            unit_serials: Vec::new(),
            wal_pages: 128,
            now: 1_767_225_600,
        };
        let mut store = Store::format(
            vec![Box::new(d) as Box<dyn BlockDev>],
            &params,
            StoreConfig::default(),
        )
        .unwrap();
        let (qsys, lib) = {
            let mut t = store.begin().unwrap();
            let qsys = ops::bootstrap(&mut t, 1_767_225_600, "QS400").unwrap();
            let lib = ops::create(
                &mut t,
                &CreateParams {
                    library: qsys,
                    name: Name::parse("QSTEST").unwrap(),
                    type_code: types::LIB,
                    attribute: String::new(),
                    text: "Test library".into(),
                    owner: "QSECOFR".into(),
                    system: "QS400".into(),
                    now: 1_767_225_600,
                    by_system: false,
                },
            )
            .unwrap();
            t.commit().unwrap();
            (qsys, lib)
        };
        let mut ctx = SqlContext::new(
            qsys,
            JobHandle(1),
            NameContext {
                curlib: Some(lib),
                libl: vec![lib],
            },
        );
        ctx.now = Some(1_767_225_600);
        ctx.current_schema = Some(Name::parse("QSTEST").unwrap());
        let mut db = Self { store, ctx };
        db.ok("create table q3t (id int not null, grp char(1), v dec(7,2), d date, s varchar(20))");
        db.ok("insert into q3t values (1,'A',10.50,'2024-01-15','apple'),(2,'A',20.00,'2024-02-29','banana'),(3,'B',null,'2023-12-31','cherry'),(4,'B',5.25,'2024-03-01',null),(5,'C',7.00,null,'date'),(6,null,1.00,'2024-01-01','fig')");
        db.ok("create table q3u (id int, w int)");
        db.ok("insert into q3u values (1,100),(3,300),(7,700),(null,0)");
        db
    }

    fn run(&mut self, text: &str) -> Result<Outcome, SqlError> {
        let p = prepare(text, Naming::Sql)?;
        let mut t = self.store.begin().unwrap();
        let r = p.execute(&mut t, &mut self.ctx, &[]);
        match &r {
            Ok(_) => t.commit().unwrap(),
            Err(_) => t.abort(),
        }
        r
    }

    fn ok(&mut self, text: &str) -> Outcome {
        self.run(text).unwrap_or_else(|e| panic!("{text}: {e}"))
    }

    fn code(&mut self, text: &str) -> i32 {
        match self.run(text) {
            Ok(o) => panic!("{text}: no error but {o:?}"),
            Err(e) => e.sqlcode(),
        }
    }

    /// Rows as text, `-` for null, joined by `|`.
    fn rows(&mut self, text: &str) -> Vec<String> {
        match self.ok(text) {
            Outcome::Rows(rs) => rs
                .rows
                .iter()
                .map(|r| {
                    r.iter()
                        .map(|v| {
                            if v.is_null() {
                                "-".into()
                            } else {
                                v.to_string().trim_end().to_string()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("|")
                })
                .collect(),
            o => panic!("{text}: {o:?}"),
        }
    }

    /// The result columns' types.
    fn types(&mut self, text: &str) -> Vec<String> {
        match self.ok(text) {
            Outcome::Rows(rs) => rs
                .columns
                .iter()
                .map(|c| {
                    format!(
                        "{}{}",
                        c.data_type.replace(' ', ""),
                        if c.nullable { "" } else { " NN" }
                    )
                })
                .collect(),
            o => panic!("{text}: {o:?}"),
        }
    }
}

fn v(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| (*s).to_string()).collect()
}

const NONE: &[&str] = &[];

#[test]
fn common_table_expressions() {
    let mut db = Db::new();
    assert_eq!(
        db.rows("with t(n) as (select 1 from sysibm.sysdummy1 union all select n+1 from t where n < 5) select * from t"),
        v(&["1", "2", "3", "4", "5"])
    );
    assert_eq!(
        db.rows("with recursive t(n) as (select 1 from sysibm.sysdummy1 union all select n+1 from t where n < 3) select * from t"),
        v(&["1", "2", "3"])
    );
    assert_eq!(
        db.rows("with a as (select id from q3t), b as (select id from a where id > 3) select * from b order by id"),
        v(&["4", "5", "6"])
    );
    assert_eq!(
        db.rows("with t(n) as (select 1 from sysibm.sysdummy1 union all select mod(n+1,3) from t where n < 10) cycle n set cyc to '1' default '0' select * from t"),
        v(&["1", "2", "0", "1"])
    );
    assert_eq!(
        db.rows("with t(n) as (select 1 from sysibm.sysdummy1 union all select mod(n+1,3) from t where n < 10) cycle n set cyc to '1' default '0' select n, cyc from t"),
        v(&["1|0", "2|0", "0|0", "1|1"])
    );
    db.ok("create table tree (id int, parent int)");
    db.ok("insert into tree values (1, null), (2, 1), (3, 1), (4, 2), (5, 3)");
    assert_eq!(
        db.rows("with t(id, lvl) as (select id, 0 from tree where parent is null union all select c.id, t.lvl + 1 from tree c, t where c.parent = t.id) search depth first by id set seq select id, lvl from t order by seq"),
        v(&["1|0", "2|1", "4|2", "3|1", "5|2"])
    );
    assert_eq!(
        db.rows("with t(id, lvl) as (select id, 0 from tree where parent is null union all select c.id, t.lvl + 1 from tree c join t on c.parent = t.id) search breadth first by id set seq select id from t order by seq"),
        v(&["1", "2", "3", "4", "5"])
    );
}

#[test]
fn joins_and_derived_tables() {
    let mut db = Db::new();
    assert_eq!(
        db.rows("select a.id, b.id, b.w from q3t a full outer join q3u b on a.id = b.id order by a.id, b.id"),
        v(&["1|1|100", "2|-|-", "3|3|300", "4|-|-", "5|-|-", "6|-|-", "-|7|700", "-|-|0"])
    );
    assert_eq!(
        db.rows("select * from q3t a join q3u b using (id)"),
        v(&[
            "1|A|10.50|2024-01-15|apple|100",
            "3|B|-|2023-12-31|cherry|300"
        ])
    );
    assert_eq!(
        db.rows("select id, grp, w from q3t join q3u using (id)"),
        v(&["1|A|100", "3|B|300"])
    );
    assert_eq!(
        db.rows("select a.id, b.id from q3t a join q3u b using (id) where a.id = 3"),
        v(&["3|3"])
    );
    assert_eq!(
        db.rows("select a.id, x.c from q3t a, lateral (select count(*) c from q3u b where b.id <= a.id) x order by a.id"),
        v(&["1|1", "2|1", "3|2", "4|2", "5|2", "6|2"])
    );
    assert_eq!(
        db.rows("select a.id, x.c from q3t a, table (select count(*) c from q3u b where b.id <= a.id) x order by a.id"),
        v(&["1|1", "2|1", "3|2", "4|2", "5|2", "6|2"])
    );
    assert_eq!(
        db.rows("select a.id, x.c from q3t a cross join lateral (select max(w) c from q3u b where b.id = a.id) x order by a.id"),
        v(&["1|100", "2|-", "3|300", "4|-", "5|-", "6|-"])
    );
    assert_eq!(
        db.rows("select a.id, x.c from q3t a left join lateral (select w c from q3u b where b.id = a.id) x on 1=1 order by a.id"),
        v(&["1|100", "2|-", "3|300", "4|-", "5|-", "6|-"])
    );
    assert_eq!(
        db.rows("select * from (select id from q3t order by id desc fetch first 2 rows only) x"),
        v(&["6", "5"])
    );
    assert_eq!(
        db.rows(
            "select id from q3t where id in (select id from q3u order by w fetch first 2 rows only)"
        ),
        v(&["1"])
    );
    assert_eq!(
        db.rows("select id from (select id, v from q3t order by v) x order by order of x"),
        v(&["6", "4", "5", "1", "2", "3"])
    );
    assert_eq!(
        db.rows("select x.* from table(systools.split('a,b,c', ',')) x"),
        v(&["1|a", "2|b", "3|c"])
    );
    // A view runs as a derived table: its order column stays hidden.
    db.ok("create view q3v as select id, grp from q3t");
    assert_eq!(
        db.rows("select * from q3v where id < 3"),
        v(&["1|A", "2|A"])
    );
    assert_eq!(db.rows("select id from (select id from q3v order by id desc) x order by order of x fetch first 1 rows only"), v(&["6"]));
    assert_eq!(
        db.rows("select v.id, u.w from q3v v join q3u u using (id) order by 1"),
        v(&["1|100", "3|300"])
    );
    // JSON_TABLE is a table function (JSON1 on pub400).
    assert_eq!(
        db.rows(r#"select t.* from json_table('{"r":[{"a":1,"b":"x"},{"a":2}]}', '$.r[*]' columns(a integer path '$.a', b varchar(5) path '$.b', n for ordinality)) as t"#),
        v(&["1|x|1", "2|-|2"])
    );
    // The JSON aggregates are a family of the aggregates.
    assert_eq!(
        db.rows("select grp, json_arrayagg(id order by id desc) from q3t where grp is not null group by grp order by grp"),
        v(&["A|[2,1]", "B|[4,3]", "C|[5]"])
    );
}

#[test]
fn row_limits_and_order() {
    let mut db = Db::new();
    assert_eq!(
        db.rows("select id from q3t order by id offset 2 rows"),
        v(&["3", "4", "5", "6"])
    );
    assert_eq!(
        db.rows("select id from q3t order by id offset 2 rows fetch next 2 rows only"),
        v(&["3", "4"])
    );
    assert_eq!(
        db.rows("select id from q3t order by id limit 2 offset 3"),
        v(&["4", "5"])
    );
    assert_eq!(
        db.rows("select id from q3t order by id limit 3,2"),
        v(&["4", "5"])
    );
    assert_eq!(
        db.code("select id from q3t order by id fetch first 50 percent rows only"),
        -104
    );
    assert_eq!(
        db.code("select id, v from q3t order by v nulls first"),
        -199
    );
    assert_eq!(
        db.rows("select id, v from q3t order by 2, 1"),
        v(&["6|1.00", "4|5.25", "5|7.00", "1|10.50", "2|20.00", "3|-"])
    );
    assert_eq!(db.code("select id, v from q3t order by 3"), -125);
}

#[test]
fn olap_specifications() {
    let mut db = Db::new();
    assert_eq!(
        db.rows("select id, row_number() over (order by id desc) rn from q3t order by id"),
        v(&["1|6", "2|5", "3|4", "4|3", "5|2", "6|1"])
    );
    assert_eq!(
        db.rows("select id, grp, row_number() over (partition by grp order by id) rn, rank() over (order by grp) rk, dense_rank() over (order by grp) dr from q3t order by id"),
        v(&["1|A|1|1|1", "2|A|2|1|1", "3|B|1|3|2", "4|B|2|3|2", "5|C|1|5|3", "6|-|1|6|4"])
    );
    assert_eq!(
        db.rows("select id, ntile(4) over (order by id) nt, percent_rank() over (order by id) pr, cume_dist() over (order by id) cd from q3t order by id"),
        v(&[
            "1|1|0|0.1666666666666666666666666666666667",
            "2|1|0.2|0.3333333333333333333333333333333333",
            "3|2|0.4|0.5",
            "4|2|0.6|0.6666666666666666666666666666666667",
            "5|3|0.8|0.8333333333333333333333333333333333",
            "6|4|1|1",
        ])
    );
    assert_eq!(
        db.rows("select id, lag(id) over (order by id) l1, lag(id, 2, -1) over (order by id) l2, lead(v) over (order by id) ld from q3t order by id"),
        v(&["1|-|-1|20.00", "2|1|-1|-", "3|2|1|5.25", "4|3|2|7.00", "5|4|3|1.00", "6|5|4|-"])
    );
    assert_eq!(
        db.rows("select id, first_value(id) over (partition by grp order by id) fv, last_value(id) over (partition by grp order by id) lv, last_value(id) over (partition by grp order by id rows between unbounded preceding and unbounded following) lv2 from q3t order by id"),
        v(&["1|1|1|2", "2|1|2|2", "3|3|3|4", "4|3|4|4", "5|5|5|5", "6|6|6|6"])
    );
    assert_eq!(
        db.rows("select nth_value(id, 2) over (order by id rows between unbounded preceding and unbounded following) nv from q3t"),
        v(&["2", "2", "2", "2", "2", "2"])
    );
    assert_eq!(
        db.rows("select id, sum(v) over (order by id rows between 1 preceding and 1 following) s3, avg(v) over (partition by grp) a, count(*) over () c from q3t order by id"),
        v(&[
            "1|30.50|15.25000000000000000000000000|6",
            "2|30.50|15.25000000000000000000000000|6",
            "3|25.25|5.25000000000000000000000000|6",
            "4|12.25|5.25000000000000000000000000|6",
            "5|13.25|7.00000000000000000000000000|6",
            "6|8.00|1.00000000000000000000000000|6",
        ])
    );
    assert_eq!(
        db.rows("select id, sum(id) over (order by id range between 1 preceding and current row) r from q3t order by id"),
        v(&["1|1", "2|3", "3|5", "4|7", "5|9", "6|11"])
    );
    assert_eq!(
        db.rows("select id, sum(id) over (order by id) cum, max(id) over (order by id rows 2 preceding) m from q3t order by id"),
        v(&["1|1|1", "2|3|2", "3|6|3", "4|10|4", "5|15|5", "6|21|6"])
    );
    assert_eq!(
        db.rows("select first_value(v) ignore nulls over (order by id) from q3t")[0],
        "10.50"
    );
    assert_eq!(
        db.rows("select ratio_to_report(id) over () from q3t")[0],
        "0.04761904761904761904761904761904762"
    );
    assert_eq!(
        db.rows("select id from q3t order by row_number() over (order by id desc) fetch first 2 rows only"),
        v(&["6", "5"])
    );
    assert_eq!(
        db.rows("select grp, sum(id), rank() over (order by sum(id) desc) from q3t group by grp order by 3"),
        v(&["B|7|1", "-|6|2", "C|5|3", "A|3|4"])
    );
    assert_eq!(
        db.code("select id from q3t where row_number() over (order by id) = 1"),
        -120
    );
    assert_eq!(
        db.types("select row_number() over (order by id), rank() over (order by id), ntile(2) over (order by id), cume_dist() over (order by id), percent_rank() over (order by id), lag(v) over (order by id) from q3t"),
        v(&["BIGINT NN", "BIGINT NN", "BIGINT", "DECFLOAT(34) NN", "DECFLOAT(34)", "DECIMAL(7,2)"])
    );
}

#[test]
fn grouping_sets() {
    let mut db = Db::new();
    assert_eq!(
        db.rows(
            "select grp, count(*) c from q3t group by grouping sets ((grp), ()) order by grp, c"
        ),
        v(&["A|2", "B|2", "C|1", "-|1", "-|6"])
    );
    assert_eq!(
        db.rows("select grp, year(d) y, sum(id) s, grouping(grp) g1, grouping(year(d)) g2 from q3t group by rollup (grp, year(d)) order by grp, y, s"),
        v(&[
            "A|2024|3|0|0",
            "A|-|3|0|1",
            "B|2023|3|0|0",
            "B|2024|4|0|0",
            "B|-|7|0|1",
            "C|-|5|0|0",
            "C|-|5|0|1",
            "-|2024|6|0|0",
            "-|-|6|0|1",
            "-|-|21|1|1",
        ])
    );
    assert_eq!(
        db.rows("select grp, year(d) y, count(*) c from q3t group by cube (grp, year(d))")
            .len(),
        13
    );
    assert_eq!(
        db.rows("select grp, s, count(*) from q3t group by grouping sets (grp, s, ())")
            .len(),
        11
    );
    assert_eq!(
        db.types("select grp, grouping(grp), count(*) from q3t group by rollup(grp)"),
        v(&["CHAR(1)", "SMALLINT", "INTEGER NN"])
    );
}

#[test]
fn aggregates() {
    let mut db = Db::new();
    assert_eq!(
        db.rows("select stddev(v), stddev_samp(v), stddev_pop(v), variance(v), var(v), var_samp(v), var_pop(v), variance_samp(v) from q3t"),
        v(&["6.4031242374328485|7.158910531638177|6.4031242374328485|41|41|51.25|41|51.25"])
    );
    assert_eq!(
        db.rows("select stddev(id), variance(id), var_samp(id) from q3t"),
        v(&["1.707825127659933|2.916666666666666|3.5"])
    );
    assert_eq!(
        db.rows("select median(v), median(id), percentile_cont(0.5) within group (order by v), percentile_disc(0.5) within group (order by v), percentile_cont(0.25) within group (order by id desc) from q3t"),
        v(&["7.00|3.5|7.00|7.00|2.25"])
    );
    assert_eq!(
        db.rows("select grp, listagg(s, ',') within group (order by s desc) from q3t group by grp order by grp"),
        v(&["A|banana,apple", "B|cherry", "C|date", "-|fig"])
    );
    assert_eq!(
        db.rows("select listagg(s) from q3t"),
        v(&["applebananacherrydatefig"])
    );
    assert_eq!(
        db.rows("select listagg(distinct grp, '-') within group (order by grp) from q3t"),
        v(&["A-B-C"])
    );
    assert_eq!(
        db.rows("select listagg(s, ',' on overflow truncate '...' with count) from q3t"),
        v(&["apple,banana,cherry,date,fig"])
    );
    assert_eq!(
        db.rows("select covariance(id, v), covariance_samp(id, v), correlation(id, v) from q3t"),
        v(&["-9|-11.25|-0.7578292430727833"])
    );
    assert_eq!(
        db.rows("select regr_slope(v, id), regr_intercept(v, id), regr_count(v, id), regr_r2(v, id), regr_avgx(v, id), regr_avgy(v, id), regr_sxx(v, id), regr_syy(v, id), regr_sxy(v, id) from q3t"),
        v(&["-2.616279069767442|18.16860465116279|5|0.5743051616562677|3.6|8.75|17.200000000000003|205|-45"])
    );
    assert_eq!(
        db.rows("select count(distinct grp), sum(distinct v), avg(distinct id), min(distinct v), max(all v), count_big(distinct s) from q3t"),
        v(&["3|43.75|3|1.00|20.00|5"])
    );
    assert_eq!(
        db.types("select count(distinct grp), sum(distinct v), avg(distinct id), min(distinct v), count_big(distinct s), stddev(v), median(v), percentile_disc(0.5) within group (order by v), listagg(s), regr_count(v, id) from q3t"),
        v(&["INTEGER NN", "DECIMAL(31,2)", "INTEGER", "DECIMAL(7,2)", "BIGINT NN", "DOUBLE", "DECFLOAT(34)", "DECIMAL(7,2)", "VARCHAR(4000)", "BIGINT"])
    );
    assert_eq!(db.code("select array_agg(id) from q3t"), -20_441);
    assert_eq!(db.code("select xmlagg(id) from q3t"), -84);
}

#[test]
fn predicates() {
    let mut db = Db::new();
    assert_eq!(
        db.rows("select id from q3t where (id, grp) in (values (1,'A'),(3,'B'))"),
        v(&["1", "3"])
    );
    assert_eq!(
        db.rows("select id from q3t where (id, grp) in (select id, 'B' from q3u)"),
        v(&["3"])
    );
    assert_eq!(
        db.rows("select id from q3t where (id, grp) = (3, 'B')"),
        v(&["3"])
    );
    assert_eq!(
        db.code("select id from q3t where (id, grp) > (3, 'B')"),
        -115
    );
    assert_eq!(
        db.rows("select id from q3t where (id, grp) <> (3, 'B')"),
        v(&["1", "2", "4", "5", "6"])
    );
    assert_eq!(
        db.rows("select id from q3t where grp is distinct from 'A'"),
        v(&["3", "4", "5", "6"])
    );
    assert_eq!(
        db.rows("select id from q3t where grp is not distinct from 'A'"),
        v(&["1", "2"])
    );
    assert_eq!(
        db.code("select id from q3t where grp is not distinct from null"),
        -206
    );
    assert_eq!(
        db.rows("select id from q3t where s like 'a%' escape '\\'"),
        v(&["1"])
    );
    assert_eq!(
        db.rows("select id from q3t where s like '%\\_%' escape '\\'"),
        v(NONE)
    );
    assert_eq!(
        db.rows("select id from q3t where id between 1+1 and 2*2"),
        v(&["2", "3", "4"])
    );
    assert_eq!(
        db.rows("select id from q3t where 3 between id and id+1"),
        v(&["2", "3"])
    );
    assert_eq!(
        db.rows("select id from q3t a where exists (select 1 from q3u b where b.id = a.id and exists (select 1 from q3t c where c.id = b.id and exists (select 1 from q3u d where d.w = b.w and a.id < 5)))"),
        v(&["1", "3"])
    );
    assert_eq!(
        db.rows("select id from q3t where id not in (select id from q3u)"),
        v(NONE)
    );
    assert_eq!(
        db.rows("select id from q3t where id not in (select id from q3u where id is not null)"),
        v(&["2", "4", "5", "6"])
    );
}

#[test]
fn date_arithmetic() {
    let mut db = Db::new();
    assert_eq!(
        db.rows("values (date('2024-03-01') - date('2023-01-31'), date('2024-01-31') - date('2024-03-01'))"),
        v(&["10101|-101"])
    );
    assert_eq!(
        db.rows(
            "values (time('10:30:15') - time('08:45:50'), time('08:00:00') - time('09:30:01'))"
        ),
        v(&["14425|-13001"])
    );
    assert_eq!(
        db.rows("values timestamp('2024-03-01-10.30.15.000005') - timestamp('2023-01-31-12.00.00.000010')"),
        v(&["10100223014.999995"])
    );
    assert_eq!(
        db.types("values (date('2024-03-01') - date('2023-01-31'), time('10:30:15') - time('08:45:50'), timestamp('2024-03-01-10.30.15') - timestamp('2023-01-31-12.00.00'))"),
        v(&["DECIMAL(8,0) NN", "DECIMAL(6,0) NN", "DECIMAL(20,6) NN"])
    );
    assert_eq!(db.code("select d + 1 from q3t where id = 2"), -182);
    assert_eq!(db.code("values date('2024-01-01') + 1 second"), -187);
    assert_eq!(db.code("values time('10:00:00') + 1 day"), -187);
    assert_eq!(
        db.rows("values (timestamp('2024-01-01-00.00.00') + 1 microsecond, timestamp('2024-01-01-00.00.00') + 3 microseconds - 1 second)"),
        v(&["2024-01-01 00:00:00.000001|2023-12-31 23:59:59.000003"])
    );
    assert_eq!(
        db.rows("values (time('23:59:59') + 2 seconds, time('10:00:00') + 1 hour + 30 minutes)"),
        v(&["00.00.01|11.30.00"])
    );
    assert_eq!(
        db.rows("values (date('2024-03-31') - 1 month, date('2024-02-29') + 1 year)"),
        v(&["2024-02-29|2025-02-28"])
    );
    assert_eq!(
        db.rows("values timestamp('2024-01-01-10.00.00') - 1 day - 0.5 seconds"),
        v(&["2023-12-31 09:59:59.500000"])
    );
    assert_eq!(
        db.rows("select d + 2.7 days from q3t where id = 1"),
        v(&["2024-01-17"])
    );
    assert_eq!(
        db.rows("values timestamp('2024-01-01-10.00.00') + 1.5 hours"),
        v(&["2024-01-01 11:00:00.000000"])
    );
    assert_eq!(
        db.rows("values timestamp('2024-01-01-10.00.00.123456789012', 12) + 1 microsecond"),
        v(&["2024-01-01 10:00:00.123457789012"])
    );
}

#[test]
fn data_changes_as_tables() {
    let mut db = Db::new();
    assert_eq!(
        db.rows("select id, w from final table (insert into q3u values (8, 800), (9, 900))"),
        v(&["8|800", "9|900"])
    );
    assert_eq!(
        db.rows("select id from new table (insert into q3u values (10, 1000))"),
        v(&["10"])
    );
    // Db2 for i 7.5: FINAL and NEW TABLE of an INSERT only (pub400).
    assert_eq!(
        db.code("select id, w from final table (update q3u set w = w + 1 where id >= 8)"),
        -199
    );
    assert_eq!(
        db.code("select id from final table (delete from q3u where id = 1)"),
        -199
    );
    assert_eq!(
        db.code("select id from old table (delete from q3u where id >= 8)"),
        -199
    );
    // MERGE from a joined source.
    db.ok("merge into q3u u using q3t t join q3u x on t.id = x.id on u.id = t.id when matched then update set w = 1");
    assert_eq!(
        db.rows("select id, w from q3u where id in (1, 3) order by id"),
        v(&["1|1", "3|1"])
    );
    assert_eq!(db.code("with t(n) as (select 1 from sysibm.sysdummy1 union select n+1 from t where n < 3) select * from t"), -342);
    assert_eq!(
        db.code("with t(n) as (select n+1 from t where n < 3) select * from t"),
        -346
    );
    assert_eq!(
        db.code("update q3u set w = q3t.id from q3t where q3u.id = q3t.id"),
        -199
    );
    db.ok("truncate table q3u");
    assert_eq!(db.rows("select count(*) from q3u"), v(&["0"]));
    db.ok("truncate q3u reuse storage ignore delete triggers continue identity immediate");
}

#[test]
fn registers_and_clauses() {
    let mut db = Db::new();
    assert_eq!(db.rows("values current degree"), v(&["NONE"]));
    db.ok("set current degree = 'ANY'");
    assert_eq!(db.rows("values current degree"), v(&["ANY"]));
    assert_eq!(
        db.rows("values current decfloat rounding mode"),
        v(&["ROUND_HALF_EVEN"])
    );
    db.ok("set current decfloat rounding mode = round_half_up");
    assert_eq!(
        db.rows("values current decfloat rounding mode"),
        v(&["ROUND_HALF_UP"])
    );
    assert_eq!(db.rows("values current client_applname"), v(&[""]));
    assert_eq!(db.rows("values qsys2.job_name"), v(&["-"]));
    db.ok("set path = system path, myschema");
    assert_eq!(
        db.rows("values current path"),
        v(&["\"QSYS\",\"QSYS2\",\"SYSPROC\",\"SYSIBMADM\",\"MYSCHEMA\""])
    );
    assert_eq!(
        db.code("select * from q3t with rs use and keep exclusive locks"),
        -84
    );
    assert_eq!(db.code("select * from q3t wait for outcome"), -84);
    assert_eq!(
        db.rows("select id from q3t where id = 1 skip locked data"),
        v(&["1"])
    );
    assert_eq!(
        db.rows("select id from q3t where id = 1 optimize for 1 row"),
        v(&["1"])
    );
}
