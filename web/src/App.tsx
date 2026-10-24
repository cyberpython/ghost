import { useState } from 'react'
import { Routes, Route } from 'react-router'
import List from './pages/List.tsx'
import Detail from './pages/Detail.tsx'
export default function App(){
  const [auto,setAuto]=useState<boolean>(()=>localStorage.getItem('ghost-auto') !== '0')
  return (
    <Routes>
      <Route path="/" element={<List auto={auto} onAuto={setAuto} />} />
      <Route path="/captures/:id" element={<Detail auto={auto} onAuto={setAuto} />} />
    </Routes>
  )
}
