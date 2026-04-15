import React, { useState, useEffect, useMemo } from 'react';
import {
  BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
  Cell, ComposedChart, Line, Area
} from 'recharts';
import {
  Zap, ShieldAlert, Coins, Layers, ArrowDownWideNarrow,
  Activity, Timer, ChevronRight, CheckCircle2, AlertTriangle
} from 'lucide-react';

const App = () => {
  // Параметры системы
  const [config, setConfig] = useState({
    targetVolume: 500,     // Желаемый объем выкупа (контрактов)
    bookDepth: 0.8,        // Ликвидность (чем меньше, тем быстрее растет цена от объема)
    commission: 0.015,     // 1.5% комиссия
    threshold: 0.04,       // Защитный порог $0.04
    networkLag: 400,       // Лаг в мс
    volatility: 0.02       // Риск изменения цены во время лага
  });

  const [market, setMarket] = useState({ yesPrice: 0.44, noPrice: 0.48 });
  const [executionResult, setExecutionResult] = useState(null);
  const [isProcessing, setIsProcessing] = useState(false);

  // Живой рынок
  useEffect(() => {
    const timer = setInterval(() => {
      if (isProcessing) return;
      setMarket(prev => ({
        yesPrice: Math.max(0.4, Math.min(0.55, prev.yesPrice + (Math.random() - 0.5) * 0.005)),
        noPrice: Math.max(0.4, Math.min(0.55, prev.noPrice + (Math.random() - 0.5) * 0.005))
      }));
    }, 1500);
    return () => clearInterval(timer);
  }, [isProcessing]);

  // Расчет "Водопада Арбитража"
  const pipeline = useMemo(() => {
    // 1. Теоретический спред (Best Ask)
    const rawSpread = 1.0 - (market.yesPrice + market.noPrice);

    // 2. Влияние VWAP (Ликвидность)
    const slippageFactor = (1 - config.bookDepth) * 0.0001;
    const vwapYes = market.yesPrice + (config.targetVolume * slippageFactor / 2);
    const vwapNo = market.noPrice + (config.targetVolume * slippageFactor / 2);
    const vwapImpact = (vwapYes + vwapNo) - (market.yesPrice + market.noPrice);
    const profitAfterVWAP = rawSpread - vwapImpact;

    // 3. Комиссии
    const feeImpact = (vwapYes + vwapNo) * config.commission;
    const profitAfterFees = profitAfterVWAP - feeImpact;

    // 4. Защитный порог (Threshold)
    const finalBuffer = profitAfterFees - config.threshold;

    return [
      { name: 'Raw Spread', value: rawSpread, color: '#3b82f6', desc: 'Теоретическая разница цен' },
      { name: 'VWAP Loss', value: -vwapImpact, color: '#ef4444', desc: 'Проскальзывание в стакане' },
      { name: 'Fees', value: -feeImpact, color: '#f59e0b', desc: 'Комиссия площадки' },
      { name: 'Threshold', value: -config.threshold, color: '#8b5cf6', desc: 'Запас на неатомарность' },
      { name: 'Net Profit', value: finalBuffer, color: finalBuffer > 0 ? '#10b981' : '#64748b', desc: 'Ожидаемый результат' }
    ];
  }, [market, config]);

  const canExecute = pipeline[4].value > 0;

  const handleExecute = async () => {
    setIsProcessing(true);
    setExecutionResult(null);

    const startPrice = market.yesPrice;

    await new Promise(r => setTimeout(r, config.networkLag));

    const slippageEvent = (Math.random() - 0.4) * config.volatility;
    const finalNoPrice = market.noPrice + slippageEvent;

    const realProfit = pipeline[4].value - slippageEvent;

    setExecutionResult({
      success: realProfit > 0,
      value: realProfit,
      slippage: slippageEvent
    });
    setIsProcessing(false);
  };

  return (
    <div className="min-h-screen bg-gray-100 p-4 md:p-8 font-sans">
      <div className="max-w-6xl mx-auto space-y-6">

        {/* Header */}
        <div className="bg-white p-6 rounded-2xl shadow-sm border border-gray-200">
          <div className="flex justify-between items-center">
            <div>
              <h1 className="text-2xl font-black text-gray-800 uppercase tracking-tight flex items-center gap-2">
                <Layers className="text-blue-600" /> Pipeline Арбитража
              </h1>
              <p className="text-gray-500 text-sm">От сырого спреда до чистого профита: как «умирает» сделка</p>
            </div>
            <div className="flex gap-4">
              <div className="text-right">
                <span className="text-[10px] font-bold text-gray-400 uppercase">Yes Ask</span>
                <div className="text-xl font-mono font-bold text-blue-600">${market.yesPrice.toFixed(3)}</div>
              </div>
              <div className="text-right">
                <span className="text-[10px] font-bold text-gray-400 uppercase">No Ask</span>
                <div className="text-xl font-mono font-bold text-red-500">${market.noPrice.toFixed(3)}</div>
              </div>
            </div>
          </div>
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-12 gap-6">

          {/* Controls */}
          <div className="lg:col-span-4 space-y-4">
            <div className="bg-white p-5 rounded-2xl shadow-sm border border-gray-200">
              <h3 className="text-sm font-bold text-gray-700 mb-4 flex items-center gap-2 uppercase tracking-wider">
                <Activity size={16} /> Рыночные условия
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

          {/* Visualization Waterfall */}
          <div className="lg:col-span-8 bg-white p-6 rounded-2xl shadow-sm border border-gray-200">
            <h3 className="text-sm font-bold text-gray-700 mb-6 uppercase tracking-wider">Водопад прибыли на акцию ($1.00)</h3>
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

            <div className="mt-6 grid grid-cols-2 md:grid-cols-4 gap-4">
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

        {/* Execution Results */}
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
                Проскальзывание второй ноги составило <span className="font-bold">${executionResult.slippage.toFixed(4)}</span>.
                {executionResult.success
                  ? ' Ваш защитный порог (Threshold) выдержал удар.'
                  : ' Проскальзывание оказалось сильнее вашего порога прибыли.'}
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
