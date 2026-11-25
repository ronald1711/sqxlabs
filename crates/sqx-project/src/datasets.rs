//! Immutable imported datasets, independent of any strategy or engine build.
use serde::{Serialize,Deserialize};
use sqx_data::SymbolSeries;
use crate::archive::Archive;

#[derive(Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {schema:u32,series:SymbolSeries}
#[derive(Serialize)]
pub struct DatasetInfo {
    pub id:String,pub symbol:String,pub timeframe:String,pub bars:usize,
    pub first:Option<i64>,pub last:Option<i64>,pub normalization_supplied:bool,
}

pub struct Datasets {archive:Archive}
impl Datasets {
    pub fn new(root:impl Into<std::path::PathBuf>)->Self {Self{archive:Archive::new(root)}}
    fn validate(series:&SymbolSeries)->Result<(),String>{
        series.validate_calendar_context()?;
        if series.symbol.trim().is_empty(){return Err("Saved dataset requires a symbol".into());}
        let quality=sqx_data::quality::audit(&series.bars,series.timeframe,sqx_data::quality::now_ms(),series.calendar.as_ref());
        if !quality.errors.is_empty(){return Err(quality.errors.join("; "));}
        if let Some(bundle)=&series.normalization {
            let duration=match series.timeframe {
                sqx_core::Timeframe::Tick|sqx_core::Timeframe::MN1=>None,
                tf=>tf.minutes().and_then(|n|n.checked_mul(60000)),
            };
            bundle.verify_bars(&series.bars,duration)?;
        }
        Ok(())
    }
    pub fn save(&self,series:&SymbolSeries)->Result<String,String>{
        Self::validate(series)?;
        let bytes=serde_json::to_vec(&Snapshot{schema:1,series:series.clone()}).map_err(|e|e.to_string())?;
        self.archive.write_blob("imports",&bytes)
    }
    pub fn load(&self,id:&str)->Result<SymbolSeries,String>{
        let snapshot:Snapshot=serde_json::from_slice(&self.archive.read_blob("imports",id)?).map_err(|e|e.to_string())?;
        if snapshot.schema!=1{return Err("Unsupported saved dataset schema".into());}
        Self::validate(&snapshot.series)?;
        Ok(snapshot.series)
    }
    pub fn list(&self)->Result<Vec<DatasetInfo>,String>{
        let directory=self.archive.root().join("imports");
        if !directory.exists(){return Ok(Vec::new());}
        let mut result=Vec::new();
        for entry in std::fs::read_dir(directory).map_err(|e|e.to_string())? {
            let path=entry.map_err(|e|e.to_string())?.path();
            if path.extension().and_then(|v|v.to_str())!=Some("json"){continue;}
            let id=path.file_stem().and_then(|v|v.to_str()).ok_or("Invalid saved dataset filename")?;
            let series=self.load(id)?;
            result.push(DatasetInfo{id:id.to_owned(),symbol:series.symbol,timeframe:series.timeframe.label().into(),bars:series.bars.len(),first:series.bars.first().map(|b|b.time),last:series.bars.last().map(|b|b.time),normalization_supplied:series.normalization.is_some()});
        }
        result.sort_by(|a,b|a.id.cmp(&b.id));Ok(result)
    }
}
