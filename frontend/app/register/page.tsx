'use client';

import { useState } from 'react';
import { useRouter } from 'next/navigation';
import Link from 'next/link';
import { useAuth } from '@/lib/api';
import { authApi } from '@/lib/api';
import toast from 'react-hot-toast';
import { Cloud, FileCheck, Download, Shield, Zap, Check } from 'lucide-react';

export default function RegisterPage() {
  const router = useRouter();
  const { login } = useAuth();
  const [activeTab, setActiveTab] = useState<'password' | 'code'>('password');
  const [username, setUsername] = useState('');
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  const [loading, setLoading] = useState(false);
  const [showPassword, setShowPassword] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (password !== confirmPassword) {
      toast.error('两次输入的密码不一致');
      return;
    }

    if (password.length < 8) {
      toast.error('密码长度至少为 8 个字符');
      return;
    }

    setLoading(true);

    try {
      const response = await authApi.register({ username, email, password });
      const { token, user } = response.data;
      
      login(token, user);
      
      toast.success('注册成功！');
      router.push('/');
    } catch (error: any) {
      const message = error.response?.data?.error || '注册失败，请检查您的信息';
      toast.error(message);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="min-h-screen bg-white flex">
      {/* 左侧蓝色区域 */}
      <div className="w-1/2 bg-gradient-to-br from-blue-600 to-blue-400 flex items-center justify-center p-12 relative overflow-hidden">
        {/* 背景装饰圆圈 */}
        <div className="absolute top-10 left-10 w-64 h-64 bg-white/10 rounded-full blur-3xl"></div>
        <div className="absolute bottom-20 right-20 w-96 h-96 bg-white/10 rounded-full blur-3xl"></div>
        <div className="absolute top-1/2 left-1/4 w-48 h-48 bg-white/10 rounded-full blur-2xl"></div>

        <div className="relative z-10 max-w-md">
          {/* 插图区域 */}
          <div className="text-center mb-12">
            <div className="relative inline-block mb-8">
              <div className="w-48 h-48 bg-white/20 rounded-3xl flex items-center justify-center backdrop-blur-sm">
                <Cloud className="w-24 h-24 text-white/90" />
              </div>
              <div className="absolute -bottom-4 -right-4 w-32 h-24 bg-white/25 rounded-2xl flex items-center justify-center backdrop-blur-sm">
                <FileCheck className="w-16 h-16 text-white/90" />
              </div>
            </div>
            
            <h1 className="text-4xl font-bold text-white mb-4">立即加入</h1>
            <p className="text-white/80 text-lg">开始您的云存储之旅，享受无限可能</p>
          </div>

          {/* 特性列表 */}
          <div className="space-y-4">
            <div className="flex items-center space-x-3 text-white/90">
              <div className="w-6 h-6 bg-white/20 rounded-full flex items-center justify-center">
                <Check className="w-4 h-4" />
              </div>
              <span>完全免费，注册即用、零门槛上手</span>
            </div>
            <div className="flex items-center space-x-3 text-white/90">
              <div className="w-6 h-6 bg-white/20 rounded-full flex items-center justify-center">
                <Check className="w-4 h-4" />
              </div>
              <span>永久直链，链接生成后长期有效不过期</span>
            </div>
            <div className="flex items-center space-x-3 text-white/90">
              <div className="w-6 h-6 bg-white/20 rounded-full flex items-center justify-center">
                <Check className="w-4 h-4" />
              </div>
              <span>直链下载，免登录、不限速，对外即用</span>
            </div>
          </div>
        </div>
      </div>

      {/* 右侧注册表单区域 */}
      <div className="w-1/2 flex items-center justify-center p-12">
        <div className="w-full max-w-md">
          {/* 顶部 Logo */}
          <div className="mb-12">
            <div className="flex items-center space-x-2">
              <div className="w-10 h-10 bg-gradient-to-br from-blue-600 to-blue-400 rounded-xl flex items-center justify-center">
                <Zap className="w-6 h-6 text-white" />
              </div>
              <span className="text-2xl font-bold text-gray-900">个人笔记</span>
            </div>
          </div>

          {/* 欢迎信息 */}
          <div className="mb-8">
            <h2 className="text-2xl font-bold text-gray-900 mb-2">创建账号</h2>
            <p className="text-gray-500">填写以下信息完成注册</p>
          </div>

          {/* 登录方式切换 */}
          <div className="flex justify-center mb-8">
            <div className="bg-gray-100 rounded-lg p-1 flex">
              <button
                onClick={() => setActiveTab('password')}
                className={`px-6 py-2 rounded-md text-sm font-medium transition-all ${
                  activeTab === 'password'
                    ? 'bg-white text-blue-600 shadow-sm'
                    : 'text-gray-500 hover:text-gray-700'
                }`}
              >
                账号密码
              </button>
              <button
                onClick={() => setActiveTab('code')}
                className={`px-6 py-2 rounded-md text-sm font-medium transition-all ${
                  activeTab === 'code'
                    ? 'bg-white text-blue-600 shadow-sm'
                    : 'text-gray-500 hover:text-gray-700'
                }`}
              >
                邮箱验证码
              </button>
            </div>
          </div>

          {/* 注册表单 */}
          <form onSubmit={handleSubmit} className="space-y-4">
            {activeTab === 'password' ? (
              <>
                <div>
                  <input
                    type="text"
                    placeholder="请输入用户名"
                    value={username}
                    onChange={(e) => setUsername(e.target.value)}
                    className="w-full px-4 py-3 border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent transition-all"
                    required
                    minLength={3}
                    maxLength={50}
                  />
                </div>
                <div>
                  <input
                    type="email"
                    placeholder="请输入邮箱"
                    value={email}
                    onChange={(e) => setEmail(e.target.value)}
                    className="w-full px-4 py-3 border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent transition-all"
                    required
                  />
                </div>
                <div className="relative">
                  <input
                    type={showPassword ? 'text' : 'password'}
                    placeholder="请输入密码（至少 8 个字符）"
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                    className="w-full px-4 py-3 border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent transition-all pr-12"
                    required
                    minLength={8}
                  />
                  <button
                    type="button"
                    onClick={() => setShowPassword(!showPassword)}
                    className="absolute right-3 top-1/2 -translate-y-1/2 text-gray-400 hover:text-gray-600"
                  >
                    {showPassword ? (
                      <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21" />
                      </svg>
                    ) : (
                      <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                      </svg>
                    )}
                  </button>
                </div>
                <div>
                  <input
                    type={showPassword ? 'text' : 'password'}
                    placeholder="确认密码"
                    value={confirmPassword}
                    onChange={(e) => setConfirmPassword(e.target.value)}
                    className="w-full px-4 py-3 border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent transition-all"
                    required
                    minLength={8}
                  />
                </div>
              </>
            ) : (
              <>
                <div>
                  <input
                    type="text"
                    placeholder="请输入用户名"
                    className="w-full px-4 py-3 border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent transition-all"
                    required
                    minLength={3}
                    maxLength={50}
                  />
                </div>
                <div>
                  <input
                    type="email"
                    placeholder="请输入邮箱"
                    className="w-full px-4 py-3 border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent transition-all"
                    required
                  />
                </div>
                <div className="flex space-x-2">
                  <input
                    type="text"
                    placeholder="请输入验证码"
                    className="flex-1 px-4 py-3 border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent transition-all"
                    required
                  />
                  <button
                    type="button"
                    className="px-6 py-3 border border-gray-200 rounded-lg text-gray-600 hover:bg-gray-50 transition-all"
                  >
                    获取验证码
                  </button>
                </div>
                <div>
                  <input
                    type="password"
                    placeholder="请输入密码（至少 8 个字符）"
                    className="w-full px-4 py-3 border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent transition-all"
                    required
                    minLength={8}
                  />
                </div>
              </>
            )}

            <button
              type="submit"
              disabled={loading}
              className="w-full py-3 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded-lg transition-all disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {loading ? '注册中...' : '立即注册'}
            </button>
          </form>

          {/* 登录链接 */}
          <div className="text-center mt-6">
            <span className="text-gray-500 text-sm">已有账号？</span>
            <Link href="/login" className="text-blue-600 hover:text-blue-700 text-sm font-medium ml-1">
              立即登录
            </Link>
          </div>

          {/* 第三方登录 */}
          <div className="mt-8">
            <div className="relative">
              <div className="absolute inset-0 flex items-center">
                <div className="w-full border-t border-gray-200"></div>
              </div>
              <div className="relative flex justify-center text-sm">
                <span className="px-4 bg-white text-gray-500">其他登录方式</span>
              </div>
            </div>

            <div className="mt-6 grid grid-cols-2 gap-3">
              <button className="flex items-center justify-center space-x-2 px-4 py-2 border border-gray-200 rounded-lg hover:bg-gray-50 transition-all">
                <div className="w-5 h-5 bg-gradient-to-br from-yellow-400 to-orange-500 rounded-full"></div>
                <span className="text-sm text-gray-600">LinuxDo 登录</span>
              </button>
              <button className="flex items-center justify-center space-x-2 px-4 py-2 border border-gray-200 rounded-lg hover:bg-gray-50 transition-all">
                <div className="w-5 h-5 bg-blue-500 rounded-full"></div>
                <span className="text-sm text-gray-600">QQ 登录</span>
              </button>
            </div>
          </div>

          {/* 安全提示 */}
          <div className="mt-8 p-4 bg-blue-50 rounded-lg border border-blue-100">
            <div className="flex items-start space-x-2">
              <Shield className="w-5 h-5 text-blue-600 mt-0.5" />
              <div className="text-sm text-blue-800">
                <p className="font-medium mb-1">安全承诺</p>
                <p className="text-blue-600">您的数据将受到加密保护，我们承诺不会未经授权使用您的文件</p>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
