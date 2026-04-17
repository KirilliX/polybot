import React, { useState, useEffect, useMemo, useCallback } from 'react';
import {
  BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, Cell
} from 'recharts';
import {
  Zap, Layers, Activity, CheckCircle2, AlertTriangle, WifiOff, Clock
} from 'lucide-react';

const GAMMA_API = '/api/gamma';
const CLOB_API  = '/api/clob';

// Шаг 1: получить token IDs для текущего BTC 15-min рынка из Gamma API
async function fetchBtcMarketInfo() {
  const res = await fetch(`${GAMMA_API}/events?limit=200`);
  if (!res.ok) throw new Error(`Gamma API ${res.status}`);
  const events = await res.json();

  const now = Date.now();
  const btc = events
    .filter(e =>
      e.slug && e.slug.includes('btc-updown-15m') &&
      e.markets && e.markets.length >= 2
    )
    .sort((a, b) => new Date(a.endDate) - new Date(b.endDate));

  if (!btc.length) throw new Error('Нет BTC 15-min рынков в Gamma API');

  // Предпочитаем ещё не закрытые; если все закрыты — берём последний (стакан ещё живой)
  const active = btc.filter(e => new Date(e.endDate).getTime() > now);
  const ev = active.length ? active[0] : btc[btc.length - 1];

  let upMkt = null, downMkt = null;
  for (const m of ev.markets) {
    const q = (m.question || m.slug || '').toLowerCase();
    if (q.includes('up')) upMkt = m;
    else if (q.includes('down')) downMkt = m;
  }
  if (!upMkt)   upMkt   = ev.markets[0];
  if (!downMkt) downMkt = ev.markets[1];

  const parseTokenId = (m) => {
    try {
      const arr = typeof m.clobTokenIds === 'string'
        ? JSON.parse(m.clobTokenIds)
        : (m.clobTokenIds || []);
      return arr[0] || null;
    } catch { return null; }
  };

  const parseFallbackPrice = (m) => {
    try {
      const arr = typeof m.outcomePrices === 'string'
        ? JSON.parse(m.outcomePrices) : m.outcomePrices;
      const v = parseFloat(arr[0]);
      return isNaN(v) ? 0.5 : v;
    } catch { return 0.5; }
  };

  return {
    upTokenId:     parseTokenId(upMkt),
    downTokenId:   parseTokenId(downMkt),
    upPriceFallback:   parseFallbackPrice(upMkt),
    downPriceFallback: parseFallbackPrice(downMkt),
    title:   ev.title || ev.slug || 'BTC Up or Down',
    endTime: new Date(ev.endDate).getTime(),
    slug:    ev.slug,
  };
}

// Шаг 2: best-ask из стакана CLOB /book (как в Rust-боте)
// Asks возвращаются DESC (0.99 первый) → берём минимум
async function fetchClobPrice(tokenId) {
  if (!tokenId) return null;
  try {
    const res = await fetch(`${CLOB_API}/book?token_id=${tokenId}`);
    if (!res.ok) return null;
    const data = await res.json();
    const asks = Array.isArray(data.asks) ? data.asks : [];
    const prices = asks
      .map(o => parseFloat(o.price))
      .filter(p => !isNaN(p) && p > 0.01 && p < 0.99);
    if (!prices.length) {
      // Фоллбэк на last_trade_price если стакан пустой
      const ltp = parseFloat(data.last_trade_price);
      return (!isNaN(ltp) && ltp > 0.01 && ltp < 0.99) ? ltp : null;
    }
    return Math.min(...prices);
  } catch { return null; }
}

function formatTtl(ms) {
  if (ms <= 0) return '00:00';
  const total = Math.floor(ms / 1000);
  const m = Math.floor(total / 60).toString().padStart(2, '0');
  const s = (total % 60).toString().padStart(2, '0');
  return `${m}:${s}`;
}

const App = () => {
  const [config, setConfig] = useState({
    targetVolume: 500,
    bookDepth:    0.8,
    takerFee:     0.02,
    threshold:    0.04,
    networkLag:   400,
    volatility:   0.02,
  });

  const [market, setMarket]         = useState({ yesPrice: 0.50, noPrice: 0.50 });
  const [marketInfo, setMarketInfo] = useState(null);
  const [tokenIds, setTokenIds]     = useState({ up: null, down: null });
  const [ttl, setTtl]               = useState(null);
  const [liveStatus, setLiveStatus] = useState('loading');
  const [fetchError, setFetchError] = useState(null);
  const [executionResult, setExecutionResult] = useState(null);
  const [isProcessing, setIsProcessing]       = useState(false);

  // Шаг 1: каждые 60с обновляем информацию о рынке и token IDs
  const loadMarketInfo = useCallback(async () => {
    try {
      const data = await fetchBtcMarketInfo();
      setMarketInfo({ title: data.title, endTime: data.endTime, slug: data.slug });
      setTokenIds({ up: data.upTokenId, down: data.downTokenId });
      // Фолбэк на цены из Gamma если CLOB ещё не загрузился
      setMarket(prev =>
        prev.yesPrice === 0.5 && prev.noPrice === 0.5
          ? { yesPrice: data.upPriceFallback, noPrice: data.downPriceFallback }
          : prev
      );
      setFetchError(null);
    } catch (err) {
      setLiveStatus('error');
      setFetchError(err.message);
    }
  }, []);

  useEffect(() => {
    loadMarketInfo();
    const t = setInterval(loadMarketInfo, 60_000);
    return () => clearInterval(t);
  }, [loadMarketInfo]);

  // Шаг 2: каждые 5с тянем живые цены из CLOB API (best-ask из стакана)
  useEffect(() => {
    if (!tokenIds.up && !tokenIds.down) return;
    const poll = async () => {
      const [up, down] = await Promise.all([
        fetchClobPrice(tokenIds.up),
        fetchClobPrice(tokenIds.down),
      ]);
      if (up !== null || down !== null) {
        setMarket({
          yesPrice: up   ?? market.yesPrice,
          noPrice:  down ?? market.noPrice,
        });
        setLiveStatus('live');
      }
    };
    poll();
    const t = setInterval(poll, 5_000);
    return () => clearInterval(t);
  }, [tokenIds.up, tokenIds.down]);

  useEffect(() => {
    if (!marketInfo?.endTime) return;
    setTtl(Math.max(0, marketInfo.endTime - Date.now()));
    const t = setInterval(() => {
      setTtl(prev => Math.max(0, prev - 1000));
    }, 1000);
    return () => clearInterval(t);
  }, [marketInfo?.endTime]);

  const pipeline = useMemo(() => {
    const rawSpread = 1.0 - (market.yesPrice + market.noPrice);

    const slippageFactor = (1 - config.bookDepth) * 0.0001;
    const vwapYes = market.yesPrice + (config.targetVolume * slippageFactor / 2);
    const vwapNo  = market.noPrice  + (config.targetVolume * slippageFactor / 2);
    const vwapImpact = (vwapYes + vwapNo) - (market.yesPrice + market.noPrice);
    const profitAfterVWAP = rawSpread - vwapImpact;

    const feeImpact = (vwapYes + vwapNo) * config.takerFee * 2;
    const profitAfterFees = profitAfterVWAP - feeImpact;

    const finalBuffer = profitAfterFees - config.threshold;

    return [
      { name: 'Raw Spread', value: rawSpread,        color: '#3b82f6', desc: 'Теоретическая разница цен' },
      { name: 'VWAP Loss',  value: -vwapImpact,      color: '#ef4444', desc: 'Проскальзывание в стакане' },
      { name: 'Fees ×2',    value: -feeImpact,       color: '#f59e0b', desc: `Taker fee ${(config.takerFee*100).toFixed(1)}% × 2 лега` },
      { name: 'Threshold',  value: -config.threshold, color: '#8b5cf6', desc: 'Запас на неатомарность' },
      { name: 'Net Profit', value: finalBuffer,       color: finalBuffer > 0 ? '#10b981' : '#64748b', desc: 'Ожидаемый результат' },
    ];
  }, [market, config]);

  const canExecute = pipeline[4].value > 0;

  const handleExecute = async () => {
    setIsProcessing(true);
    setExecutionResult(null);
    await new Promise(r => setTimeout(r, config.networkLag));
    const slippageEvent = (Math.random() - 0.4) * config.volatility;
    const realProfit = pipeline[4].value - slippageEvent;
    setExecutionResult({ success: realProfit > 0, value: realProfit, slippage: slippageEvent });
    setIsProcessing(false);
  };

  return (
    <div className="min-h-screen bg-gray-100 p-4 md:p-8 font-sans">
      <div className="max-w-6xl mx-auto space-y-6">

        {fetchError && (
          <div className="bg-red-50 border border-red-200 rounded-xl px-4 py-3 flex items-center gap-3 text-sm text-red-700">
            <WifiOff size={16} className="shrink-0" />
            <span>API недоступен: {fetchError}. Показаны последние известные цены.</span>
          </div>
        )}

        {/* Header */}
        <div className="bg-white p-6 rounded-2xl shadow-sm border border-gray-200">
          <div className="flex flex-col sm:flex-row justify-between items-start sm:items-center gap-3">
            <div>
              <h1 className="text-2xl font-black text-gray-800 uppercase tracking-tight flex items-center gap-2">
                <Layers className="text-blue-600" /> Pipeline Арбитража
              </h1>
              <p className="text-gray-500 text-sm mt-0.5 flex items-center gap-2">
                {liveStatus === 'live' ? (
                  <span className="flex items-center gap-1.5 text-green-600 font-semibold">
                    <span className="relative flex h-2 w-2">
                      <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
                      <span className="relative inline-flex rounded-full h-2 w-2 bg-green-500"></span>
                    </span>
                    LIVE
                  </span>
                ) : liveStatus === 'loading' ? (
                  <span className="text-yellow-600 font-semibold">Загрузка...</span>
                ) : (
                  <span className="flex items-center gap-1 text-red-500 font-semibold">
                    <WifiOff size={12}/> OFFLINE
                  </span>
                )}
                {marketInfo && <span className="text-gray-400 truncate max-w-xs">{marketInfo.title}</span>}
              </p>
            </div>
            <div className="flex items-center gap-4 flex-wrap">
              {ttl !== null && (
                <div className="flex items-center gap-1.5 bg-gray-900 text-white px-3 py-1.5 rounded-lg">
                  <Clock size={13} className="text-gray-400" />
                  <span className="font-mono font-bold text-sm">{formatTtl(ttl)}</span>
                </div>
              )}
              <div className="text-right">
                <span className="text-[10px] font-bold text-gray-400 uppercase">Up Ask</span>
                <div className="text-xl font-mono font-bold text-blue-600">${market.yesPrice.toFixed(3)}</div>
              </div>
              <div className="text-right">
                <span className="text-[10px] font-bold text-gray-400 uppercase">Down Ask</span>
                <div className="text-xl font-mono font-bold text-red-500">${market.noPrice.toFixed(3)}</div>
              </div>
              <div className="text-right">
                <span className="text-[10px] font-bold text-gray-400 uppercase">Сумма</span>
                <div className={`text-xl font-mono font-bold ${(market.yesPrice + market.noPrice) < 1.0 ? 'text-green-600' : 'text-gray-400'}`}>
                  ${(market.yesPrice + market.noPrice).toFixed(3)}
                </div>
              </div>
            </div>
          </div>
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-12 gap-6">

          {/* Controls */}
          <div className="lg:col-span-4 space-y-4">
            <div className="bg-white p-5 rounded-2xl shadow-sm border border-gray-200">
              <h3 className="text-sm font-bold text-gray-700 mb-4 flex items-center gap-2 uppercase tracking-wider">
                <Activity size={16} /> Параметры
              </h3>
              <div className="space-y-5">

                <div>
                  <div className="flex justify-between text-xs mb-2">
                    <span className="text-gray-500">Объем сделки</span>
                    <span className="font-bold">{config.targetVolume} контрактов</span>
                  </div>
                  <input type="range" min="10" max="2000" step="10" value={config.targetVolume}
                    onChange={e => setConfig({...config, targetVolume: parseInt(e.target.value)})}
                    className="w-full h-1.5 bg-gray-200 rounded-lg appearance-none cursor-pointer accent-blue-600" />
                </div>

                <div>
                  <div className="flex justify-between text-xs mb-2">
                    <span className="text-gray-500">Глубина ликвидности</span>
                    <span className="font-bold">{(config.bookDepth * 100).toFixed(0)}%</span>
                  </div>
                  <input type="range" min="0.1" max="1" step="0.1" value={config.bookDepth}
                    onChange={e => setConfig({...config, bookDepth: parseFloat(e.target.value)})}
                    className="w-full h-1.5 bg-gray-200 rounded-lg appearance-none cursor-pointer accent-red-500" />
                </div>

                <div className="pt-2 border-t border-gray-100">
                  <div className="flex justify-between text-xs mb-2">
                    <span className="text-gray-500">Taker Fee (каждый лег)</span>
                    <span className="font-bold text-yellow-600">{(config.takerFee * 100).toFixed(1)}%</span>
                  </div>
                  <input type="range" min="0" max="0.05" step="0.001" value={config.takerFee}
                    onChange={e => setConfig({...config, takerFee: parseFloat(e.target.value)})}
                    className="w-full h-1.5 bg-gray-200 rounded-lg appearance-none cursor-pointer accent-yellow-500" />
                  <div className="flex justify-between text-[10px] text-gray-400 mt-1">
                    <span>0%</span><span>↑ Polymarket ~2%</span><span>5%</span>
                  </div>
                </div>

                <div className="pt-2 border-t border-gray-100">
                  <div className="flex justify-between text-xs mb-2">
                    <span className="text-gray-500">Защитный порог (Risk Buffer)</span>
                    <span className="font-bold text-purple-600">${config.threshold.toFixed(2)}</span>
                  </div>
                  <input type="range" min="0" max="0.1" step="0.01" value={config.threshold}
                    onChange={e => setConfig({...config, threshold: parseFloat(e.target.value)})}
                    className="w-full h-1.5 bg-gray-200 rounded-lg appearance-none cursor-pointer accent-purple-600" />
                </div>

              </div>
            </div>

            <div className={`p-5 rounded-2xl shadow-lg transition-all border-2 ${canExecute ? 'bg-green-600 border-green-400' : 'bg-gray-800 border-gray-700'}`}>
              <div className="text-white">
                <h4 className="text-xs font-bold opacity-80 uppercase mb-1">Статус Триггера</h4>
                <div className="flex justify-between items-end">
                  <div className="text-2xl font-black">
                    {canExecute ? 'READY TO SEND' : 'WAITING...'}
                  </div>
                  {canExecute && <Zap className="text-yellow-300 animate-pulse" size={24} fill="currentColor" />}
                </div>
                <button
                  disabled={!canExecute || isProcessing}
                  onClick={handleExecute}
                  className={`w-full mt-4 py-3 rounded-xl font-bold transition-all shadow-inner uppercase tracking-widest text-sm ${
                    canExecute
                    ? 'bg-white text-green-700 hover:bg-green-50 active:scale-95'
                    : 'bg-gray-700 text-gray-500 cursor-not-allowed'
                  }`}
                >
                  {isProcessing ? 'Исполнение...' : 'Открыть позицию'}
                </button>
              </div>
            </div>
          </div>

          {/* Waterfall */}
          <div className="lg:col-span-8 bg-white p-6 rounded-2xl shadow-sm border border-gray-200">
            <h3 className="text-sm font-bold text-gray-700 mb-6 uppercase tracking-wider">
              Водопад прибыли на акцию ($1.00)
              <span className="ml-2 text-[10px] text-gray-400 normal-case font-normal">
                fee {(config.takerFee*100).toFixed(1)}% × 2 лега
              </span>
            </h3>
            <div className="h-80 w-full">
              <ResponsiveContainer width="100%" height="100%">
                <BarChart data={pipeline} margin={{ top: 20, right: 30, left: 0, bottom: 0 }}>
                  <CartesianGrid strokeDasharray="3 3" vertical={false} stroke="#f1f5f9" />
                  <XAxis dataKey="name" axisLine={false} tickLine={false} tick={{fontSize: 10, fontWeight: 700}} />
                  <YAxis axisLine={false} tickLine={false} tick={{fontSize: 10}} domain={[-0.1, 0.15]} />
                  <Tooltip
                    cursor={{fill: '#f8fafc'}}
                    content={({ active, payload }) => {
                      if (active && payload && payload.length) {
                        return (
                          <div className="bg-white p-3 shadow-xl border border-gray-100 rounded-lg">
                            <p className="text-xs font-bold text-gray-800">{payload[0].payload.name}</p>
                            <p className="text-lg font-mono font-bold" style={{color: payload[0].payload.color}}>
                              {payload[0].value > 0 ? '+' : ''}{payload[0].value.toFixed(4)}
                            </p>
                            <p className="text-[10px] text-gray-400 mt-1">{payload[0].payload.desc}</p>
                          </div>
                        );
                      }
                      return null;
                    }}
                  />
                  <Bar dataKey="value" radius={[4, 4, 0, 0]}>
                    {pipeline.map((entry, index) => (
                      <Cell key={`cell-${index}`} fill={entry.color} />
                    ))}
                  </Bar>
                </BarChart>
              </ResponsiveContainer>
            </div>

            <div className="mt-6 grid grid-cols-2 md:grid-cols-5 gap-3">
              {pipeline.map((item, i) => (
                <div key={i} className="bg-gray-50 p-3 rounded-xl border border-gray-100">
                  <div className="text-[10px] font-bold text-gray-400 uppercase leading-tight">{item.name}</div>
                  <div className="text-sm font-mono font-bold mt-1" style={{color: item.color}}>
                    {item.value > 0 ? '+' : ''}{item.value.toFixed(3)}
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>

        {executionResult && (
          <div className={`p-6 rounded-2xl border-2 flex flex-col md:flex-row items-center gap-6 ${
            executionResult.success ? 'bg-green-50 border-green-200' : 'bg-red-50 border-red-200'
          }`}>
            <div className={`w-16 h-16 rounded-full flex items-center justify-center shrink-0 ${
              executionResult.success ? 'bg-green-100 text-green-600' : 'bg-red-100 text-red-600'
            }`}>
              {executionResult.success ? <CheckCircle2 size={32} /> : <AlertTriangle size={32} />}
            </div>
            <div className="flex-1 text-center md:text-left">
              <h3 className="text-xl font-bold text-gray-800">
                {executionResult.success ? 'Успешный арбитраж!' : 'Убыточная сделка'}
              </h3>
              <p className="text-sm text-gray-600">
                Проскальзывание второй ноги: <span className="font-bold">${executionResult.slippage.toFixed(4)}</span>.
                {executionResult.success
                  ? ' Порог выдержал удар.'
                  : ' Проскальзывание сильнее порога прибыли.'}
              </p>
            </div>
            <div className="text-center md:text-right">
              <div className="text-[10px] font-bold text-gray-400 uppercase">Итоговый PnL</div>
              <div className={`text-3xl font-black font-mono ${executionResult.success ? 'text-green-600' : 'text-red-600'}`}>
                ${executionResult.value.toFixed(3)}
              </div>
            </div>
          </div>
        )}

      </div>
    </div>
  );
};

export default App;
